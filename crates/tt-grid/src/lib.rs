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

/// `BuffXMax` (`buffer.c:82`), which is `TermWidthMax` — **1000**, not the 500
/// this used to say. 500 is `TermHeightMax`, one line below it in
/// `tttypes.h:633`, and taking the wrong one made a 640-column terminal
/// silently half the width it asked for.
pub const BUFF_X_MAX: usize = 1000;

/// `ts.ScrollBuffMax`'s default (`ttset.c:1213`, key `MaxBuffSize`), which is
/// the cap `BuffChangeTerminalSize` puts on the *height* (`buffer.c:4977`) —
/// a different quantity from [`Grid::scrollback_max`], which is how deep the
/// history actually goes. Conflating them means a terminal with its scroll
/// buffer turned off can only be one row tall.
pub const MAX_ROWS_DEFAULT: usize = 10_000;

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

/// `vtterm.c:SaveCursorBuf` — everything DECSC puts away. The two modes come
/// with it and are easy to miss: restoring a cursor also restores **autowrap**
/// and **origin mode**, so `ESC 7` … `ESC [ ? 7 l` … `ESC 8` leaves autowrap
/// *on*. The pending wrap is deliberately absent, because upstream restores the
/// position through `MoveCursor`, which clears it.
#[derive(Clone, Copy, Debug)]
struct SavedCursor {
    x: usize,
    y: usize,
    pen: Pen,
    origin_mode: bool,
    autowrap: bool,
}

pub type Line = Vec<Cell>;

pub struct Grid {
    cols: usize,
    rows: usize,
    lines: Vec<Line>,
    scrollback: VecDeque<Line>,
    scrollback_max: usize,
    /// `ts.ScrollBuffMax` — the ceiling on the *page* height, which upstream
    /// applies at `buffer.c:4977` and which has nothing to do with how much
    /// history is kept. See [`MAX_ROWS_DEFAULT`].
    max_rows: usize,
    /// Monotonic count of lines pushed into the scrollback — see
    /// [`Grid::scrolled_off`]. Not derivable from the length, which stops
    /// moving once the buffer is full.
    scrolled_off: u64,
    pub cursor: Cursor,
    pub pen: Pen,
    /// Scroll region, 0-based and inclusive. Tera Term's `CursorTop`/`CursorBottom`.
    top: usize,
    bottom: usize,
    /// Left and right margins, 0-based and inclusive — `CursorLeftM` /
    /// `CursorRightM`. They are the screen edges until DECSLRM moves them, and
    /// they gate far more than the name suggests: the wrap point, CR, the tab
    /// stops, ICH/DCH, IL/DL, every region scroll and DECFI/DECBI all read
    /// them.
    left: usize,
    right: usize,
    pub insert_mode: bool,
    pub autowrap: bool,
    pub origin_mode: bool,
    tabs: Vec<bool>,
    /// DECSC's two slots, main screen and alternate. See [`Grid::saved_slot`].
    saved: [Option<SavedCursor>; 2],
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
            max_rows: MAX_ROWS_DEFAULT,
            scrolled_off: 0,
            cursor: Cursor::default(),
            pen,
            top: 0,
            bottom: rows - 1,
            left: 0,
            right: cols - 1,
            insert_mode: false,
            // DECAWM is on out of reset.
            autowrap: true,
            origin_mode: false,
            tabs: default_tabs(cols),
            saved: [None, None],
            stashed: None,
        }
    }

    /// `BuffSaveScreen` — park a copy of the visible page.
    pub fn save_screen(&mut self) {
        self.stashed = Some(self.lines.clone());
    }

    /// `BuffRestoreScreen` — put the parked page back. A no-op if nothing was
    /// saved, which is what keeps a stray `ESC [ ? 1047 l` harmless.
    ///
    /// It copies **into** the page rather than replacing it, clipped to
    /// `min(saved, current)` on both axes — upstream's `CopyX`/`CopyY`
    /// (`buffer.c:5423`). That is not a detail: a resize between the save and
    /// the restore leaves the two different sizes, and swapping the whole
    /// buffer in would give the grid a page with the wrong number of rows,
    /// after which the first write past the old height panics. Four escape
    /// sequences reach it — `CSI ? 1047 h`, `CSI 8 ; h ; w t`, `CSI ? 1047 l`,
    /// then any output.
    ///
    /// The kanji fixup at the end is upstream's too (`:5431`): the last copied
    /// column may hold the lead half of a wide character whose padding was not
    /// copied, so it is crushed rather than left pointing at a cell that is no
    /// longer its own.
    pub fn restore_screen(&mut self) {
        let Some(stash) = self.stashed.take() else {
            return;
        };
        let copy_y = stash.len().min(self.rows);
        for (y, src) in stash.into_iter().take(copy_y).enumerate() {
            let copy_x = src.len().min(self.cols);
            self.lines[y][..copy_x].copy_from_slice(&src[..copy_x]);
            if self.lines[y][copy_x - 1].width_class == WIDTH_WIDE {
                self.lines[y][copy_x - 1].crush();
            }
        }
    }

    /// DECALN (`ESC # 8`) — `buffer.c:BuffFillWithE`. Every cell becomes an
    /// `E` with **default** attributes, not the pen's, so it also clears the
    /// protect bit. The caller resets the margins and homes the cursor.
    pub fn fill_with_e(&mut self) {
        let default = Cell::blank(Pen::default());
        for line in &mut self.lines {
            *line = vec![
                Cell {
                    text: [b'E' as u32, 0, 0, 0],
                    ..default
                };
                self.cols
            ];
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

    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// How deep the history goes, in lines beyond the page.
    pub fn scrollback_max(&self) -> usize {
        self.scrollback_max
    }

    /// Change it — the settings dialog's `ScrollBuffSize`, applied to a
    /// running terminal.
    ///
    /// Shrinking drops the **oldest** lines, which is upstream's
    /// `ChangeBuffer` (`buffer.c:545`): it keeps the end of the buffer and
    /// throws the start away. Nothing here touches `scrolled_off` — the lines
    /// evicted were still printed, and the numbering has to stay monotonic or
    /// every held line number shifts at once.
    pub fn set_scrollback_max(&mut self, max: usize) {
        self.scrollback_max = max;
        while self.scrollback.len() > max {
            self.scrollback.pop_front();
        }
    }

    /// The ceiling on the page height — `ts.ScrollBuffMax`, `buffer.c:4977`.
    pub fn set_max_rows(&mut self, max: usize) {
        self.max_rows = max.max(1);
    }

    /// One retained line, oldest first. Always [`cols`](Grid::cols) wide —
    /// `resize` refits the scrollback alongside the page, so a viewport never
    /// has to deal with a ragged history.
    pub fn scrollback_line(&self, i: usize) -> Option<&[Cell]> {
        self.scrollback.get(i).map(|l| l.as_slice())
    }

    /// How many lines have ever left the page into the scrollback.
    ///
    /// Monotonic, and deliberately not the same thing as
    /// [`scrollback_len`](Grid::scrollback_len): it keeps counting once the
    /// buffer is full and starts evicting, which is exactly when the length
    /// stops changing.
    ///
    /// A viewport needs this because it has to be anchored to *content*.
    /// Counting back from the bottom means the view slides whenever output
    /// arrives, so a user reading a stack trace watches it walk off the
    /// screen. The difference between two readings is how far to move the
    /// offset to hold still — and it is the right answer whether the
    /// scrollback grew or evicted, since both shift the content by the same
    /// number of lines.
    pub fn scrolled_off(&self) -> u64 {
        self.scrolled_off
    }

    /// The grid's structural contract, as an assertion rather than as prose.
    ///
    /// Everything here is something another layer indexes with: the painter
    /// walks `rows` lines of `cols` cells, the C ABI hands those cells out
    /// unchanged, and `codepoints()` stops at the first zero. A violation is
    /// therefore a panic waiting for the next write, which has already happened
    /// here once — see [`restore_screen`](Grid::restore_screen).
    ///
    /// Wide-character pairing is *not* here, because upstream does not maintain
    /// it either; [`check_wide_pairs`](Grid::check_wide_pairs) has it, and says
    /// where.
    ///
    /// It is not a *behavioural* check: whether the cells hold what Tera Term
    /// would put in them is `run_diff.sh`'s question, and this deliberately
    /// says nothing about it. What this covers is the ground the differential
    /// suite cannot reach, because a stream that panics produces no dump to
    /// diff.
    ///
    /// Cheap enough to call after every chunk in a fuzz target — one pass over
    /// the page and the scrollback — and that is its main caller. See
    /// `crates/tt-fuzz/`.
    pub fn check_invariants(&self) -> Result<(), String> {
        if self.cols == 0 || self.rows == 0 {
            return Err(format!("zero-sized grid {}x{}", self.cols, self.rows));
        }
        if self.lines.len() != self.rows {
            return Err(format!(
                "page holds {} lines, rows is {}",
                self.lines.len(),
                self.rows
            ));
        }
        if self.cursor.x >= self.cols || self.cursor.y >= self.rows {
            return Err(format!(
                "cursor {},{} outside {}x{}",
                self.cursor.x, self.cursor.y, self.cols, self.rows
            ));
        }
        if self.top > self.bottom || self.bottom >= self.rows {
            return Err(format!(
                "scroll region {}..={} outside {} rows",
                self.top, self.bottom, self.rows
            ));
        }
        if self.left > self.right || self.right >= self.cols {
            return Err(format!(
                "margins {}..={} outside {} columns",
                self.left, self.right, self.cols
            ));
        }
        if self.tabs.len() != self.cols {
            return Err(format!(
                "{} tab stops for {} columns",
                self.tabs.len(),
                self.cols
            ));
        }
        if self.scrollback.len() > self.scrollback_max {
            return Err(format!(
                "scrollback holds {} lines, max is {}",
                self.scrollback.len(),
                self.scrollback_max
            ));
        }
        if (self.scrollback.len() as u64) > self.scrolled_off {
            return Err(format!(
                "scrollback holds {} lines but only {} ever left the page",
                self.scrollback.len(),
                self.scrolled_off
            ));
        }

        for (i, line) in self.scrollback.iter().enumerate() {
            check_line(line, self.cols, &format!("scrollback line {i}"))?;
        }
        for (y, line) in self.lines.iter().enumerate() {
            check_line(line, self.cols, &format!("row {y}"))?;
        }
        Ok(())
    }

    /// Every wide cell has its padding and every padding cell has its wide.
    ///
    /// Deliberately **not** part of [`check_invariants`](Grid::check_invariants),
    /// because Tera Term does not maintain it. Three paths break it upstream and
    /// are reproduced:
    ///
    /// - **DECCRA** is a bare `memcpyW` (`buffer.c:1430`) with no kanji fixup
    ///   anywhere in it or in its caller, so a rectangle whose edge cuts a wide
    ///   character leaves one half behind.
    /// - **[`restore_screen`](Grid::restore_screen)** copies
    ///   `min(saved, current)` columns, so the destination's own padding can
    ///   outlive the wide cell it belonged to. Upstream crushes the *copied*
    ///   lead (`:5431`) and not that one.
    /// - **A double-width insert** shifts by two, but upstream's guard only
    ///   crushes the lead at `LineEnd - 1` (`:3298`), so a lead two cells in
    ///   still arrives at the margin without its padding.
    ///
    /// All three were found by the property test in `crates/tt-fuzz/` and then
    /// **settled against upstream's source rather than against its output**,
    /// because the dump cannot arbitrate them: a lead with no padding still
    /// prints as one glyph in two columns and a padding cell prints as nothing,
    /// so a row whose halves have come apart renders exactly like one whose have
    /// not. `run_diff.sh` says "ok" to all of it. Reading `AttrKanji` out of the
    /// oracle does not fix that either — the bit is set on the non-insert write
    /// path and not the insert one (`Attr_Attr` is the pen's byte alone), and
    /// `BuffSetChar` never clears it, so upstream's own copy is incoherent.
    ///
    /// **This function is the only check that covers that ground**, which is why
    /// it exists as something separate rather than as a debug assertion. What is
    /// left after the three exclusions — writing, wrapping, deleting, scrolling,
    /// erasing — is ours, and two real bugs have already come out of it.
    pub fn check_wide_pairs(&self) -> Result<(), String> {
        for (y, line) in self.lines.iter().enumerate() {
            for (x, cell) in line.iter().enumerate() {
                match cell.width_class {
                    // A lead with no padding is a two-column glyph in a
                    // one-column box, drawn over whatever is next to it.
                    WIDTH_WIDE if x + 1 >= self.cols => {
                        return Err(format!("row {y} column {x}: wide cell at the right edge"))
                    }
                    WIDTH_WIDE if line[x + 1].width_class != WIDTH_PAD => {
                        return Err(format!("row {y} column {x}: wide cell with no padding"))
                    }
                    // And a padding cell with nothing to pad is a column that
                    // renders as nothing at all.
                    WIDTH_PAD if x == 0 || line[x - 1].width_class != WIDTH_WIDE => {
                        return Err(format!("row {y} column {x}: padding with no wide cell"))
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// `buffer.c:BuffChangeTerminalSize` — the resize behind XTWINOPS
    /// `CSI 8 ; h ; w t`.
    ///
    /// Content is **truncated, never reflowed**: every line keeps its first
    /// `cols` cells, a wide character cut by the new right edge is crushed, and
    /// growing pads with blanks. `TF_CLEARONRESIZE` is off by default, so the
    /// screen survives; what moves is which lines the page covers.
    ///
    /// Height is the interesting half, and upstream expresses it by sliding
    /// `PageStart` rather than by moving text:
    ///
    /// - **Shrinking** keeps the *top* rows and drops the rest — unless the
    ///   cursor would fall off the bottom, in which case the page instead ends
    ///   at the cursor's line and everything above it becomes scrollback.
    /// - **Growing** pulls lines back *out of the scrollback* to fill the new
    ///   rows, and only extends downward once the scrollback runs out.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.clamp(1, BUFF_X_MAX);
        let rows = rows.clamp(1, self.max_rows.max(1));
        if cols == self.cols && rows == self.rows {
            return;
        }
        let pen = self.pen;

        // 1. Width, over the scrollback and the page alike — `ChangeBuffer`
        //    copies both through the same `memcpyW` and the same kanji fixup.
        let fit = |line: &mut Line| {
            line.resize(cols, Cell::erased(Pen::default()));
            if line[cols - 1].width_class == WIDTH_WIDE {
                line[cols - 1].crush();
            }
        };
        for line in &mut self.scrollback {
            fit(line);
        }
        for line in &mut self.lines {
            fit(line);
        }
        // The parked main screen is deliberately *not* resized. Upstream keeps
        // it in a separate allocation with its own `SaveBuffX`/`SaveBuffY`,
        // which `BuffChangeTerminalSize` never touches; `restore_screen` is
        // what reconciles the two sizes, by clipping. Refitting it here instead
        // would work but would leave that clip unreachable, and the clip is the
        // thing keeping a restore onto a differently-sized page in bounds.

        // 2. Height, as the page sliding over the scrollback.
        let cy = self.cursor.y;
        if rows < self.rows {
            if rows > cy + 1 {
                self.lines.truncate(rows);
            } else {
                // The page ends at the cursor; everything above it scrolls off.
                let first = cy + 1 - rows;
                let scrolled: Vec<Line> = self.lines.drain(..first).collect();
                for line in scrolled {
                    self.push_scrollback(line);
                }
                self.lines.truncate(rows);
                self.cursor.y = rows - 1;
            }
        } else if rows > self.rows {
            let wanted = rows - self.rows;
            let from_scrollback = wanted.min(self.scrollback.len());
            for _ in 0..from_scrollback {
                let line = self.scrollback.pop_back().expect("checked len");
                self.lines.insert(0, line);
            }
            self.cursor.y += from_scrollback;
            for _ in 0..(wanted - from_scrollback) {
                self.lines.push(vec![Cell::erased(pen); cols]);
            }
        }

        self.cols = cols;
        self.rows = rows;

        // 3. Margins, tab stops and the cursor all go back to a known state.
        self.top = 0;
        self.bottom = rows - 1;
        self.left = 0;
        self.right = cols - 1;
        self.tabs = default_tabs(cols);
        self.cursor.x = self.cursor.x.min(cols - 1);
        self.cursor.y = self.cursor.y.min(rows - 1);
    }

    /// DECSTR — `vtterm.c:SoftReset`, the grid's share of it.
    ///
    /// Note what it does *not* do: the screen is not cleared, the cursor does
    /// not move, and autowrap is left alone. What it does do that looks odd is
    /// reload DECSC's slot with the **origin** rather than the current
    /// position, so a DECRC straight after a soft reset homes the cursor.
    pub fn soft_reset(&mut self) {
        self.insert_mode = false;
        self.origin_mode = false;
        self.top = 0;
        self.bottom = self.rows - 1;
        self.left = 0;
        self.right = self.cols - 1;
        self.pen = Pen::default();

        let cursor = self.cursor;
        self.cursor = Cursor::default();
        self.save_cursor();
        self.cursor = cursor;
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
        self.left = 0;
        self.right = self.cols - 1;
        self.insert_mode = false;
        self.autowrap = true;
        self.origin_mode = false;
        self.tabs = default_tabs(self.cols);
        self.saved = [None, None];
    }

    // --- cursor ----------------------------------------------------------

    /// `buffer.c:MoveCursor` — note it clears the pending wrap. Every cursor
    /// motion that goes through here does; scrolling does not.
    pub fn move_cursor(&mut self, x: usize, y: usize) {
        self.cursor.x = x.min(self.cols - 1);
        self.cursor.y = y.min(self.rows - 1);
        self.cursor.pending_wrap = false;
    }

    /// Cursor position for CUP/HVP, honouring origin mode. In origin mode the
    /// column is relative to the **left margin**, not to the screen.
    pub fn move_cursor_abs(&mut self, x: usize, y: usize) {
        if self.origin_mode {
            let x = (self.left + x).min(self.right);
            let y = (self.top + y).min(self.bottom);
            self.move_cursor(x, y);
        } else {
            self.move_cursor(x, y);
        }
    }

    /// CHA / HPA — `vtterm.c:CSMoveToColumnN`, the horizontal twin of
    /// `move_cursor_abs`.
    pub fn move_to_column(&mut self, x: usize) {
        if self.origin_mode {
            let x = (self.left + x).min(self.right);
            self.move_cursor(x, self.cursor.y);
        } else {
            self.move_cursor(x, self.cursor.y);
        }
    }

    /// CUU with `margins`, VPB without — `vtterm.c:CSCursorUp`, whose
    /// `AffectMargin` argument this is.
    ///
    /// The four movers come in pairs upstream for one reason: the ECMA-48
    /// "position" forms (HPR, VPR, HPB, VPB) are the same motion measured
    /// against the *page* rather than against the margins. Same clamp
    /// otherwise, so they share the code, and telling them apart matters only
    /// once a scroll region or a left/right margin exists.
    pub fn move_up(&mut self, n: usize, margins: bool) {
        // Cursor motion stops at the scroll region edge, but only if the cursor
        // started inside it.
        let limit = if margins && self.cursor.y >= self.top {
            self.top
        } else {
            0
        };
        let y = self.cursor.y.saturating_sub(n).max(limit);
        self.move_cursor(self.cursor.x, y);
    }

    /// CUD with `margins`, VPR without.
    pub fn move_down(&mut self, n: usize, margins: bool) {
        let limit = if margins && self.cursor.y <= self.bottom {
            self.bottom
        } else {
            self.rows - 1
        };
        let y = (self.cursor.y + n).min(limit);
        self.move_cursor(self.cursor.x, y);
    }

    /// CUB with `margins`, HPB without. Stops at the left margin, but only when
    /// the cursor started at or right of it — from outside, the margin is not a
    /// barrier.
    pub fn move_left(&mut self, n: usize, margins: bool) {
        let limit = if margins && self.cursor.x >= self.left {
            self.left
        } else {
            0
        };
        let x = self.cursor.x.saturating_sub(n).max(limit);
        self.move_cursor(x, self.cursor.y);
    }

    /// CUF with `margins`, HPR without. Mirror image of [`Grid::move_left`].
    pub fn move_right(&mut self, n: usize, margins: bool) {
        let limit = if margins && self.cursor.x <= self.right {
            self.right
        } else {
            self.cols - 1
        };
        let x = (self.cursor.x + n).min(limit);
        self.move_cursor(x, self.cursor.y);
    }

    /// VPA — `vtterm.c:CSMoveToLineN`, the vertical twin of
    /// [`Grid::move_to_column`], and **origin mode applies**. `y` is 0-based.
    ///
    /// Not the same as passing the row to [`Grid::move_cursor`]: under origin
    /// mode the row counts from the top margin and stops at the bottom one.
    pub fn move_to_row(&mut self, y: usize) {
        if self.origin_mode {
            let y = (self.top + y).min(self.bottom);
            self.move_cursor(self.cursor.x, y);
        } else {
            self.move_cursor(self.cursor.x, y);
        }
    }

    /// `vtterm.c:CarriageReturn`. It only moves — and therefore only clears the
    /// pending wrap — when the cursor is not already at the left margin.
    pub fn carriage_return(&mut self) {
        if self.origin_mode || self.cursor.x > self.left {
            self.move_cursor(self.left, self.cursor.y);
        } else if self.cursor.x < self.left {
            // Left of the margin, CR goes to column 0 rather than *forward* to
            // the margin.
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

    /// BS — `vtterm.c:BackSpace`. It stops dead *on* the left margin rather
    /// than stepping past it, and `TF_BACKWRAP` (off by default) is the only
    /// thing that would move it to the previous line.
    pub fn backspace(&mut self) {
        if self.cursor.x != self.left && self.cursor.x > 0 {
            self.move_cursor(self.cursor.x - 1, self.cursor.y);
        }
    }

    /// Which of the two DECSC slots is live — `SBuff1` or `SBuff3`
    /// (`vtterm.c:SaveCursor`). Upstream picks on `AltScr`, and `stashed` is
    /// `Some` for exactly as long as the alternate screen is up, so it is the
    /// same test. Sharing one slot would let a full-screen editor's `ESC 7`
    /// overwrite the position the shell underneath it is going to come back to.
    ///
    /// (Upstream has a third, `SBuff2`, for the status line. We have no status
    /// line.)
    fn saved_slot(&self) -> usize {
        usize::from(self.stashed.is_some())
    }

    pub fn save_cursor(&mut self) {
        let slot = self.saved_slot();
        self.saved[slot] = Some(SavedCursor {
            x: self.cursor.x,
            y: self.cursor.y,
            pen: self.pen,
            origin_mode: self.origin_mode,
            autowrap: self.autowrap,
        });
    }

    pub fn restore_cursor(&mut self) {
        match self.saved[self.saved_slot()] {
            Some(s) => {
                self.origin_mode = s.origin_mode;
                self.autowrap = s.autowrap;
                self.pen = s.pen;
                // Through `move_cursor`, so the pending wrap clears — which is
                // what upstream's `MoveCursor` does on the way out.
                self.move_cursor(s.x, s.y);
            }
            None => self.move_cursor(0, 0),
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

    fn tab_stops(&self) -> Vec<usize> {
        (0..self.cols).filter(|&x| self.tabs[x]).collect()
    }

    /// CHT and plain HT — `buffer.c:CursorForwardTab`.
    ///
    /// A tab that runs out of stops parks on the right margin **and arms the
    /// pending wrap**, because `ts.VTCompatTab` is off by default
    /// (`ttset.c:1343`). So a tab at the end of a line behaves like a printed
    /// character there, not like a cursor move.
    pub fn forward_tab(&mut self, n: usize) {
        let line_end = if self.cursor.x > self.right || !self.cursor_in_region() {
            self.cols - 1
        } else {
            self.right
        };
        let stops = self.tab_stops();
        let first = stops
            .iter()
            .position(|&s| s > self.cursor.x)
            .unwrap_or(stops.len());
        match stops.get(first + n.max(1) - 1) {
            Some(&s) if s <= line_end => self.move_cursor(s, self.cursor.y),
            _ => {
                self.move_cursor(line_end, self.cursor.y);
                self.cursor.pending_wrap = self.autowrap;
            }
        }
    }

    /// CBT — `buffer.c:CursorBackwardTab`.
    pub fn backward_tab(&mut self, n: usize) {
        let line_start = if self.cursor.x < self.left || !self.cursor_in_region() {
            0
        } else {
            self.left
        };
        let stops = self.tab_stops();
        let first = stops
            .iter()
            .position(|&s| s >= self.cursor.x)
            .unwrap_or(stops.len());
        let n = n.max(1);
        let target = match first.checked_sub(n).and_then(|i| stops.get(i)) {
            Some(&s) if s >= line_start => s,
            _ => line_start,
        };
        self.move_cursor(target, self.cursor.y);
    }

    fn cursor_in_region(&self) -> bool {
        (self.top..=self.bottom).contains(&self.cursor.y)
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

    // --- left/right margins ----------------------------------------------

    pub fn margins(&self) -> (usize, usize) {
        (self.left, self.right)
    }

    /// True while the margins are still the screen edges, which is the fast
    /// path every region operation takes unless DECSLRM has been used.
    fn full_width(&self) -> bool {
        self.left == 0 && self.right == self.cols - 1
    }

    /// DECSLRM (`vtterm.c:CSSetLRScrollRegion`). Rejected outright when the
    /// pair is inside out, and the cursor homes to the *screen* origin unless
    /// origin mode is on — the same shape as DECSTBM.
    pub fn set_lr_margins(&mut self, left: usize, right: usize) {
        let max = self.cols - 1;
        let (left, right) = (left.min(max), right.min(max));
        if left >= right {
            return;
        }
        self.left = left;
        self.right = right;
        if self.origin_mode {
            self.move_cursor(self.left, self.top);
        } else {
            self.move_cursor(0, 0);
        }
    }

    pub fn reset_lr_margins(&mut self) {
        self.left = 0;
        self.right = self.cols - 1;
    }

    /// `buffer.c:EraseKanjiOnLRMargin` — run before any region shift while a
    /// margin is inset, because the shift is about to move one half of a wide
    /// character and leave the other behind.
    fn erase_kanji_on_lr_margin(&mut self, y0: usize, y1: usize) {
        if self.full_width() {
            return;
        }
        let (left, right, cols) = (self.left, self.right, self.cols);
        for y in y0..=y1 {
            if left > 0 && self.lines[y][left - 1].width_class == WIDTH_WIDE {
                self.lines[y][left - 1].crush();
                self.lines[y][left].crush();
            }
            if right < cols - 1 && self.lines[y][right].width_class == WIDTH_WIDE {
                self.lines[y][right].crush();
                self.lines[y][right + 1].crush();
            }
        }
    }

    /// Move the margin columns of `[top, bottom]` by `delta` rows, filling what
    /// is vacated. Negative is downward. This is the body every region scroll
    /// shares once a margin is inset: only the columns between the margins take
    /// part, and the rest of each row stays exactly where it is.
    fn shift_region_rows(&mut self, n: usize, up: bool) {
        let pen = self.pen;
        let (top, bottom, left, right) = (self.top, self.bottom, self.left, self.right);
        let n = n.min(bottom - top + 1);
        self.erase_kanji_on_lr_margin(top, bottom);
        let blank = vec![Cell::erased(pen); right - left + 1];
        let rows: Vec<usize> = if up {
            (top..=bottom).collect()
        } else {
            (top..=bottom).rev().collect()
        };
        for y in rows {
            let src = if up {
                y.checked_add(n)
            } else {
                y.checked_sub(n)
            };
            let taken = match src {
                Some(s) if (top..=bottom).contains(&s) => self.lines[s][left..=right].to_vec(),
                _ => blank.clone(),
            };
            self.lines[y][left..=right].copy_from_slice(&taken);
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        if !self.full_width() {
            self.shift_region_rows(n, true);
            return;
        }
        let pen = self.pen;
        // `BuffScroll` keeps what it scrolls off whenever the region starts at
        // the top of the screen — the bottom margin does not have to reach the
        // last row for the scrollback to fill.
        let keep = self.top == 0;
        for _ in 0..n.min(self.bottom - self.top + 1) {
            let line = self.lines.remove(self.top);
            if keep {
                self.push_scrollback(line);
            }
            self.lines
                .insert(self.bottom, vec![Cell::erased(pen); self.cols]);
        }
    }

    pub fn scroll_down(&mut self, n: usize) {
        if !self.full_width() {
            self.shift_region_rows(n, false);
            return;
        }
        let pen = self.pen;
        for _ in 0..n.min(self.bottom - self.top + 1) {
            self.lines.remove(self.bottom);
            self.lines
                .insert(self.top, vec![Cell::erased(pen); self.cols]);
        }
    }

    fn push_scrollback(&mut self, line: Line) {
        // Counted before the early return, because `scrolled_off` names lines
        // rather than storage: `Session` measures every absolute line number
        // from it. A grid with no history that did not count would call every
        // line zero, and a selection holding a line number would follow the
        // wrong row for the rest of the session.
        self.scrolled_off += 1;
        if self.scrollback_max == 0 {
            return;
        }
        if self.scrollback.len() == self.scrollback_max {
            self.scrollback.pop_front();
        }
        self.scrollback.push_back(line);
    }

    // --- editing ---------------------------------------------------------

    /// IL — `buffer.c:BuffInsertLines`. Only acts while the cursor is inside
    /// the scroll region, and only over the margin columns.
    pub fn insert_lines(&mut self, n: usize) {
        if self.cursor.y < self.top || self.cursor.y > self.bottom {
            return;
        }
        let n = n.min(self.bottom - self.cursor.y + 1);
        if !self.full_width() {
            self.shift_lines_in_margins(self.cursor.y, n, false);
            return;
        }
        let pen = self.pen;
        for _ in 0..n {
            self.lines.remove(self.bottom);
            self.lines
                .insert(self.cursor.y, vec![Cell::erased(pen); self.cols]);
        }
    }

    /// DL — `buffer.c:BuffDeleteLines`.
    pub fn delete_lines(&mut self, n: usize) {
        if self.cursor.y < self.top || self.cursor.y > self.bottom {
            return;
        }
        let n = n.min(self.bottom - self.cursor.y + 1);
        if !self.full_width() {
            self.shift_lines_in_margins(self.cursor.y, n, true);
            return;
        }
        let pen = self.pen;
        for _ in 0..n {
            self.lines.remove(self.cursor.y);
            self.lines
                .insert(self.bottom, vec![Cell::erased(pen); self.cols]);
        }
    }

    /// IL/DL restricted to the margin columns of `[start, bottom]`.
    fn shift_lines_in_margins(&mut self, start: usize, n: usize, up: bool) {
        let pen = self.pen;
        let (bottom, left, right) = (self.bottom, self.left, self.right);
        self.erase_kanji_on_lr_margin(start, bottom);
        let blank = vec![Cell::erased(pen); right - left + 1];
        let rows: Vec<usize> = if up {
            (start..=bottom).collect()
        } else {
            (start..=bottom).rev().collect()
        };
        for y in rows {
            let src = if up {
                y.checked_add(n)
            } else {
                y.checked_sub(n)
            };
            let taken = match src {
                Some(s) if (start..=bottom).contains(&s) => self.lines[s][left..=right].to_vec(),
                _ => blank.clone(),
            };
            self.lines[y][left..=right].copy_from_slice(&taken);
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
        // Outside the margins the whole operation is refused, not clipped.
        if x < self.left || x > self.right {
            return;
        }
        let pen = self.pen;
        let right = self.right;

        // Inserting *into* the right half of a wide character kills it, and so
        // does pushing one out past the right margin.
        self.break_pad_at(y, x);
        if right < self.cols - 1 {
            self.break_lead_at(y, right);
        }

        let count = n.min(right + 1 - x);
        for _ in 0..count {
            self.lines[y].remove(right);
            self.lines[y].insert(x, Cell::erased(pen));
        }

        self.break_lead_at(y, right);
    }

    /// DCH — `buffer.c:BuffDeleteChars`.
    pub fn delete_chars(&mut self, n: usize) {
        let (x, y) = (self.cursor.x, self.cursor.y);
        if x < self.left || x > self.right {
            return;
        }
        let pen = self.pen;
        let right = self.right;
        let count = n.min(right + 1 - x);

        // Either half at the cursor, and either half at the far end of the
        // deleted run, loses its partner.
        self.break_pad_at(y, x);
        self.break_lead_at(y, x);
        if count > 1 && x + count <= right {
            self.break_pad_at(y, x + count);
        }
        if right < self.cols - 1 {
            self.break_lead_at(y, right);
        }

        for _ in 0..count {
            self.lines[y].remove(x);
            self.lines[y].insert(right, Cell::erased(pen));
        }
    }

    /// DECFI's scroll — shift the margin columns of every row of the scroll
    /// region one column left. `buffer.c:BuffScrollLeft`.
    pub fn scroll_left(&mut self, n: usize) {
        let pen = self.pen;
        let (left, right, cols) = (self.left, self.right, self.cols);
        let width = right - left + 1;
        let count = n.min(width);
        for y in self.top..=self.bottom {
            // A wide character on the right margin loses the half that sits
            // outside it; one on the left margin loses the half being shifted
            // away; and the last cell shifted out orphans its padding.
            self.break_lead_at(y, right);
            if count < width {
                self.break_pad_at(y, left + count);
            }
            if left > 0 {
                self.break_lead_at(y, left - 1);
            }
            for _ in 0..count {
                self.lines[y].remove(left);
                self.lines[y].insert(right, Cell::erased(pen));
            }
            let _ = cols;
        }
    }

    /// DECBI's scroll. `buffer.c:BuffScrollRight`.
    pub fn scroll_right(&mut self, n: usize) {
        let pen = self.pen;
        let (left, right) = (self.left, self.right);
        let count = n.min(right - left + 1);
        for y in self.top..=self.bottom {
            self.break_lead_at(y, right);
            if left > 0 {
                self.break_lead_at(y, left - 1);
            }
            for _ in 0..count {
                self.lines[y].remove(right);
                self.lines[y].insert(left, Cell::erased(pen));
            }
            self.break_lead_at(y, right);
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
        self.left = 0;
        self.right = self.cols - 1;
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
        //
        // Upstream parks it by calling `BuffPutUnicode(0x20, …)` recursively
        // (`vtterm.c:896`), so the space goes through the whole write path —
        // including the two crushes at the top of it, which break a wide
        // character the cursor is standing on *before* the overflow is even
        // detected (`buffer.c:3219`, `:3241`). Writing the cell directly skips
        // that, and the cursor is standing on a padding cell rather often here:
        // a wide glyph at the right margin leaves it there by design, and any
        // cursor motion that clears the pending wrap leaves it there with the
        // wrap gone. The result was a wide character with its right half
        // replaced by a space — half a glyph, which is the one thing this
        // branch exists to prevent.
        if w == 2 && self.cursor.x + 1 > self.cols - 1 {
            if self.autowrap {
                let (x, y) = (self.cursor.x, self.cursor.y);
                let pen = self.pen;
                self.split_wide_at(y, x);
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
            // `vtterm.c:917` — the wrap is armed at the right *margin* as well
            // as at the screen edge, so a narrow margin wraps early.
            if x == self.right || x >= self.cols - 1 {
                self.cursor.pending_wrap = self.autowrap;
            } else {
                self.cursor.x = x + 1;
                self.cursor.pending_wrap = false;
            }
        } else if x + 1 == self.right || x + 1 >= self.cols - 1 {
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
            // `buffer.c:3278` — the shift ends at the **right margin**, not at
            // the screen edge, and only a cursor that has got outside the
            // margins entirely may push the whole line. Using the edge
            // unconditionally lets a character typed inside a left/right margin
            // pair shove text out through the right one.
            let end = if x > self.right {
                self.cols - 1
            } else {
                self.right
            };
            // `buffer.c:3298` — "一番最後の文字が全角の場合", if the last
            // character is full-width. The shift pushes the cell at `end` off
            // the line, so a wide character whose padding is there would lose
            // its right half and be left as a lead alone at the margin. Break
            // it first, exactly as upstream does, rather than after the fact.
            if end > 0 {
                self.break_lead_at(y, end - 1);
            }
            for _ in 0..w.min(end + 1 - x) {
                self.lines[y].remove(end);
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

/// One line's share of [`Grid::check_invariants`]. `where` names the line for
/// the message, since a violation two hundred rows into a scrollback is
/// otherwise indistinguishable from one on the page.
fn check_line(line: &[Cell], cols: usize, wher: &str) -> Result<(), String> {
    if line.len() != cols {
        return Err(format!(
            "{wher} is {} cells wide, cols is {cols}",
            line.len()
        ));
    }
    for (x, cell) in line.iter().enumerate() {
        // A zero terminates the cell's codepoints, so a live one after it is
        // unreachable through `codepoints()` — the character is in the grid and
        // will never be drawn.
        if let Some(gap) = cell.text.iter().position(|&c| c == 0) {
            if cell.text[gap..].iter().any(|&c| c != 0) {
                return Err(format!("{wher} column {x}: text {:?} has a gap", cell.text));
            }
        }
        match cell.width_class {
            WIDTH_WIDE => {}
            WIDTH_PAD => {
                if cell.text[0] != 0 {
                    return Err(format!(
                        "{wher} column {x}: padding cell holds {:?}",
                        cell.text
                    ));
                }
            }
            WIDTH_NARROW => {
                if cell.text[0] == 0 {
                    return Err(format!("{wher} column {x}: narrow cell holds no text"));
                }
            }
            other => return Err(format!("{wher} column {x}: width class {other}")),
        }
    }
    Ok(())
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

    /// `EnableScrollBuff=off` is a terminal with no history, not a terminal
    /// one row tall — the height's ceiling is `ts.ScrollBuffMax`, a different
    /// setting entirely.
    #[test]
    fn height_is_not_bounded_by_the_history() {
        let mut g = Grid::new(80, 24, 0);
        g.resize(80, 40);
        assert_eq!(g.rows(), 40);
        g.set_max_rows(30);
        g.resize(80, 40);
        assert_eq!(g.rows(), 30);
    }

    /// Lines are counted as they leave the page whether or not anything keeps
    /// them, because the count is what every absolute line number is measured
    /// from.
    #[test]
    fn lines_are_counted_off_the_page_with_no_history_to_keep_them() {
        let mut g = Grid::new(4, 2, 0);
        for _ in 0..5 {
            g.line_feed();
        }
        assert_eq!(g.scrollback_len(), 0);
        assert_eq!(g.scrolled_off(), 4);
        g.check_invariants().unwrap();
    }

    #[test]
    fn shrinking_the_history_drops_the_oldest_lines() {
        let mut g = Grid::new(8, 1, 10);
        for c in "abcdef".chars() {
            g.cursor.x = 0;
            g.put(c as u32);
            g.line_feed();
        }
        assert_eq!(g.scrollback_len(), 6);
        g.set_scrollback_max(2);
        assert_eq!(g.scrollback_len(), 2);
        assert_eq!(g.scrollback_line(0).unwrap()[0].text[0], 'e' as u32);
        // The numbering does not move under a line somebody is holding on to.
        assert_eq!(g.scrolled_off(), 6);
        g.check_invariants().unwrap();
    }
}
