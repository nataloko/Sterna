//! Mouse and focus reporting — `vtterm.c:MouseReport` and its neighbours.
//!
//! This is the one part of the engine driven by *input events* rather than by
//! the byte stream, so it is the seam where the frontend hands us a click and
//! gets bytes for the host. The core owns the encoding for the same reason it
//! owns the keymap: which of the six wire formats is live is terminal state,
//! set by escape sequences the frontend never sees.
//!
//! Positions cross this boundary as **window pixels**, not cells. That is what
//! upstream's `MouseReport` takes, and SGR-pixel mode (`DECSET 1016`) reports
//! them back without converting, so a cell-only API could not express it.

/// `MouseReportMode` — `tttypes.h:650`.
///
/// The `repr(u32)` on this and its neighbours is not decoration: the C ABI
/// names these variants directly rather than keeping a second copy of the
/// list that can drift. See `tt-ffi`.
///
/// One variable holds two unrelated protocols: the xterm family and DEC's
/// locator. They are mutually exclusive upstream because they share this
/// field, which is why `DECELR` and `DECSET 1000` cancel each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Tracking {
    #[default]
    None,
    /// DECELR. Reports through `CSI … & w` instead of the xterm formats.
    DecLocator,
    /// `DECSET 9` — press only, no modifiers, no release.
    X10,
    /// `DECSET 1000` — press and release.
    Vt200,
    /// `DECSET 1001`. Upstream never implemented it; every path returns
    /// without reporting, and DECRQM answers 4. Kept so the mode still
    /// *displaces* whatever was active, which is observable.
    Vt200Hl,
    /// `DECSET 1002` — press, release, and motion while a button is down.
    BtnEvent,
    /// `DECSET 1003` — all of the above plus motion with no button.
    AllEvent,
    /// `DECSET 14001`. Tera Term's own: `ESC } row,col CR`, press only, and
    /// not a CSI at all.
    NetTerm,
}

/// `MouseReportExtMode` — `tttypes.h:661`. How a report is spelled, chosen
/// independently of which events are reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum Encoding {
    /// The original `CSI M` form. One byte per field, offset by 32, so it
    /// cannot express a column past 223.
    #[default]
    Normal,
    /// `DECSET 1005`.
    Utf8,
    /// `DECSET 1006`.
    Sgr,
    /// `DECSET 1015`.
    Urxvt,
    /// `DECSET 1016` — SGR again, but positions are pixels.
    SgrPixels,
}

/// `IdMouseEvent*` — `tttypes.h:668`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum MouseEvent {
    /// Not a real event: DECRQLP asks the locator where it is.
    CurStat,
    Press,
    Release,
    Move,
    /// `button` is 0 for wheel-up and 1 for wheel-down (`vtwin.cpp:2542`
    /// passes `zDelta < 0`).
    Wheel,
}

/// `IdLeftButton` … `IdButtonRelease` — `tttypes.h:674`.
pub const BUTTON_LEFT: u8 = 0;
pub const BUTTON_MIDDLE: u8 = 1;
pub const BUTTON_RIGHT: u8 = 2;
pub const BUTTON_RELEASE: u8 = 3;

/// What `ShiftKey()`/`AltKey()`/`ControlKey()` would have answered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

impl Modifiers {
    /// `vtterm.c:5667`. Note the line *above* it computes a different
    /// assignment (ctrl 8, alt 16) and is immediately overwritten — dead code,
    /// and the live one is xterm's convention.
    pub(crate) fn bits(self) -> i32 {
        (i32::from(self.shift) * 4) | (i32::from(self.alt) * 8) | (i32::from(self.ctrl) * 16)
    }
}

/// `DecLocatorFlag` — `vtterm.c:90`.
mod locator_flag {
    pub const ONE_SHOT: u32 = 1;
    pub const PIXEL: u32 = 2;
    pub const BUTTON_DOWN: u32 = 4;
    pub const BUTTON_UP: u32 = 8;
    pub const FILTERED: u32 = 16;
}

/// Everything `MouseReport` reads or writes between calls.
#[derive(Clone, Debug)]
pub struct MouseState {
    pub tracking: Tracking,
    pub encoding: Encoding,
    /// `FocusReportMode`, `DECSET 1004`.
    pub focus_report: bool,
    pub(crate) locator_flags: u32,
    /// DECEFR's rectangle, in cells, as `(top, left, bottom, right)`.
    pub(crate) filter: (i32, i32, i32, i32),
    /// `LastX`/`LastY` — the most recent position in window pixels, updated
    /// even while reporting is off.
    pub(crate) last: (i32, i32),
    /// `ButtonStat`, a bitmask the locator reports verbatim.
    pub(crate) button_stat: i32,
    /// `LastSendX`/`LastSendY`/`LastButton`. Motion is suppressed when the
    /// cell has not changed, so these are what "changed" is measured against.
    pub(crate) last_send: (i32, i32),
    pub(crate) last_button: i32,
}

impl Default for MouseState {
    /// `LastSendX`/`LastSendY`/`LastButton` are function statics initialised at
    /// `vtterm.c:5614`, and the button starts *released* — a motion event
    /// arriving before any press reports button 3, not button 0.
    fn default() -> Self {
        MouseState {
            tracking: Tracking::None,
            encoding: Encoding::Normal,
            focus_report: false,
            locator_flags: 0,
            filter: (0, 0, 0, 0),
            last: (0, 0),
            button_stat: 0,
            last_send: (-1, -1),
            last_button: BUTTON_RELEASE as i32,
        }
    }
}

impl MouseState {
    /// `ResetTerminal` (`vtterm.c:294`) clears the mode, the flags, the
    /// position and the button mask. It does **not** touch the three function
    /// statics, which are per-process rather than per-terminal; a RIS
    /// therefore leaves `LastButton` wherever the last event put it.
    /// `SoftReset` clears none of it — DECSTR deliberately leaves mouse
    /// tracking alone.
    pub(crate) fn reset(&mut self) {
        *self = MouseState {
            last_send: self.last_send,
            last_button: self.last_button,
            ..MouseState::default()
        };
    }
}

/// `MakeMouseReportStr` — `vtterm.c:5561`. The body of the report, without the
/// CSI the caller prepends.
pub(crate) fn encode(encoding: Encoding, mb: i32, x: i32, y: i32) -> Vec<u8> {
    // MOUSE_POS_LIMIT / MOUSE_POS_EXT_LIMIT.
    const LIMIT: i32 = 255 - 32;
    const EXT_LIMIT: i32 = 2047 - 32;

    match encoding {
        Encoding::Normal => {
            let x = x.min(LIMIT);
            let y = y.min(LIMIT);
            vec![b'M', (mb + 32) as u8, (x + 32) as u8, (y + 32) as u8]
        }
        Encoding::Utf8 => {
            let mut out = vec![b'M'];
            // The button byte is *not* UTF-8 encoded, even here — upstream
            // formats it with `%c` while the positions go through the encoder
            // below. With enough modifiers it exceeds 127 and the report stops
            // being valid UTF-8. Reproduced rather than corrected: it is what
            // a host talking to Tera Term sees.
            out.push((mb + 32) as u8);
            for v in [x.min(EXT_LIMIT), y.min(EXT_LIMIT)] {
                let v = v + 32;
                if v < 128 {
                    out.push(v as u8);
                } else {
                    out.push((((v >> 6) & 0x1f) | 0xc0) as u8);
                    out.push(((v & 0x3f) | 0x80) as u8);
                }
            }
            out
        }
        Encoding::Sgr | Encoding::SgrPixels => {
            let final_byte = if mb & 0x80 != 0 { 'm' } else { 'M' };
            format!("<{};{};{}{}", mb & 0x7f, x, y, final_byte).into_bytes()
        }
        Encoding::Urxvt => format!("{};{};{}M", mb + 32, x, y).into_bytes(),
    }
}

/// `MakeLocatorReportStr` — `vtterm.c:5483`. A negative `x` means the locator
/// is outside the page, which is reported as a short form with no position.
pub(crate) fn encode_locator(event: i32, button_stat: i32, x: i32, y: i32) -> Vec<u8> {
    if x < 0 {
        format!("{event};{button_stat}&w").into_bytes()
    } else {
        format!("{event};{button_stat};{y};{x};0&w").into_bytes()
    }
}

pub(crate) use locator_flag::*;
