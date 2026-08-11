//! XTWINOPS' two halves — what the window *is*, and what a host asks it to
//! become. `vtterm.c:CSSunSequence` dispatches both; `vtdisp.c` answers both.
//!
//! Neither half belongs in a VT engine, which is a function of its bytes and
//! has no window. So the engine keeps a [`WindowMetrics`] snapshot the
//! frontend refreshes, and puts what it is asked to *do* on a queue the
//! frontend drains — the same split [`crate::Vt::take_bells`] makes for the
//! same reason, and for the additional one that a report has to be answered
//! while the sequence is being parsed. A round trip to the toolkit in the
//! middle of `advance` is not available, so the answer has to be already in
//! hand.

/// What the eight window `Disp*` functions in `vtdisp.c` answer, in the units
/// they answer in.
///
/// The defaults are a **notional** window: no chrome, positioned at the
/// origin, one cell 8x16, on a 1920x1080 work area. That is not a guess about
/// anybody's desktop — it is what a terminal with no window has to say, and
/// the oracle's stubs say the same thing so the two engines can be compared on
/// the logic rather than on the numbers. A frontend that has a window replaces
/// the lot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowMetrics {
    /// Outer frame origin in screen pixels — `DispGetWindowPos(client=FALSE)`,
    /// which is `GetWindowPlacement`'s normal position, or the maximised one
    /// while maximised.
    pub pos: (i32, i32),
    /// Text-area origin in screen pixels — `DispGetWindowPos(client=TRUE)`,
    /// which is `ClientToScreen(0,0)`.
    pub client_pos: (i32, i32),
    /// Outer frame size in pixels — `DispGetWindowSize(client=FALSE)`, i.e.
    /// `GetWindowRect`. `None` means no frontend has said, and the notional
    /// window is exactly its text area.
    pub size: Option<(i32, i32)>,
    /// Text-area size in pixels — `DispGetWindowSize(client=TRUE)`. `None`
    /// derives it from the grid and [`WindowMetrics::cell`].
    pub client_size: Option<(i32, i32)>,
    /// `DispGetCellSize` — `vtdraw_t`'s `CellWidth`/`CellHeight`, which is the
    /// font's advance plus `VTFontSpace`'s margins, not the font size.
    pub cell: (i32, i32),
    /// `GetDesktopRect` (`ttlib_static.c:135`) — the **work area** of the
    /// monitor nearest the window, not the whole monitor and not the virtual
    /// desktop. Qt spells it `QScreen::availableGeometry()`.
    pub screen: (i32, i32),
    /// `DispWindowIconified` — `IsIconic`.
    pub iconified: bool,
}

impl Default for WindowMetrics {
    fn default() -> Self {
        Self {
            pos: (0, 0),
            client_pos: (0, 0),
            size: None,
            client_size: None,
            cell: (8, 16),
            screen: (1920, 1080),
            iconified: false,
        }
    }
}

impl WindowMetrics {
    /// The text area in pixels, falling back to the grid when nothing has said.
    pub(crate) fn text_area(&self, cols: usize, rows: usize) -> (i32, i32) {
        self.client_size
            .unwrap_or((cols as i32 * self.cell.0, rows as i32 * self.cell.1))
    }

    /// The outer frame in pixels, falling back to the text area — a notional
    /// window has no chrome.
    pub(crate) fn frame(&self, cols: usize, rows: usize) -> (i32, i32) {
        self.size.unwrap_or_else(|| self.text_area(cols, rows))
    }

    /// `DispGetRootWinSize(inPixels=FALSE)` (`vtdisp.c:3713`) — how many cells
    /// of *this* terminal would fit on the work area, which is the desktop
    /// less this window's own chrome, divided by the cell.
    ///
    /// Transcribed rather than simplified to `screen / cell`: with real chrome
    /// the two differ, and the subtraction is where a frontend that reports
    /// its client size wrong shows up.
    pub(crate) fn screen_cells(&self, cols: usize, rows: usize) -> (i32, i32) {
        let (fw, fh) = self.frame(cols, rows);
        let (cw, ch) = self.text_area(cols, rows);
        let div = |space: i32, chrome: i32, cell: i32| {
            if cell <= 0 {
                0
            } else {
                (space - chrome) / cell
            }
        };
        (
            div(self.screen.0, fw - cw, self.cell.0),
            div(self.screen.1, fh - ch, self.cell.1),
        )
    }
}

/// Something `CSI Ps t` asked the window to do, for a frontend to carry out.
///
/// Upstream funnels all of these through `DispShowWindow`'s `WINDOW_*` modes
/// and two direct calls; the names here are those modes. Every one of them is
/// gated on `WF_WINDOWCHANGE` before it reaches the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowRequest {
    /// `CSI 1 t` — `WINDOW_RESTORE`.
    Deiconify,
    /// `CSI 2 t` — `WINDOW_MINIMIZE`.
    Iconify,
    /// `CSI 3 ; x ; y t`. Screen pixels, and the frame rather than the text
    /// area: upstream is a bare `SetWindowPos`.
    Move(i32, i32),
    /// `CSI 4 ; height ; width t`, in pixels and reported here the other way
    /// round. **A zero or missing value means "leave that axis alone"** —
    /// `DispResizeWin` reads the current `GetWindowRect` for it
    /// (`vtdisp.c:3646`) — which is why it is 0 rather than absent.
    ResizePixels { width: i32, height: i32 },
    /// `CSI 5 t` — `WINDOW_RAISE`. Upstream deliberately does **not** take
    /// focus: `BringWindowToTop`, and a taskbar flash if that left it behind
    /// another window. The `SetForegroundWindow` alternative is in the source
    /// behind a `#if` nobody turns on.
    Raise,
    /// `CSI 6 t` — `WINDOW_LOWER`.
    Lower,
    /// `CSI 7 t` — `WINDOW_REFRESH`, an `InvalidateRect` of the whole window.
    Refresh,
    /// `CSI 9 ; 0 t` and `CSI 10 ; 0 t` — `WINDOW_RESTORE`.
    Unmaximize,
    /// `CSI 9 ; 1 t` and `CSI 10 ; 1 t` — `WINDOW_MAXIMIZE`.
    ///
    /// **`CSI 10 t` is not full screen here.** Upstream's comment says a
    /// PuTTY-style full screen is what it ought to be and that maximising is
    /// the shortcut it took instead, so the two operations are one.
    Maximize,
    /// `CSI 10 ; 2 t` — `WINDOW_TOGGLE_MAXIMIZE`. There is no `CSI 9 ; 2 t`:
    /// case 9 has arms for 0 and 1 only.
    ToggleMaximize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notional_window_is_exactly_its_text_area() {
        let m = WindowMetrics::default();
        assert_eq!(m.text_area(80, 24), (640, 384));
        assert_eq!(m.frame(80, 24), (640, 384));
        // No chrome to subtract, so the work area divides evenly.
        assert_eq!(m.screen_cells(80, 24), (240, 67));
    }

    #[test]
    fn chrome_comes_off_the_screen_before_the_cells_are_counted() {
        let m = WindowMetrics {
            size: Some((660, 420)),
            client_size: Some((640, 384)),
            ..WindowMetrics::default()
        };
        // 20px of border and 36px of caption.
        assert_eq!(m.screen_cells(80, 24), ((1920 - 20) / 8, (1080 - 36) / 16));
    }

    #[test]
    fn a_zero_cell_does_not_divide() {
        let m = WindowMetrics {
            cell: (0, 0),
            ..WindowMetrics::default()
        };
        assert_eq!(m.screen_cells(80, 24), (0, 0));
    }
}
