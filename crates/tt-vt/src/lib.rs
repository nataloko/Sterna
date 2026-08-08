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

pub mod keys;
pub mod mouse;
pub mod palette;
pub mod term_id;
pub use keys::{CrSend, Key, KeyModes};
pub use mouse::{Encoding, Modifiers, MouseEvent, Tracking};
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

#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// `WF_TITLEREPORT` (`ttset.c:1664`). Three-valued upstream and shipped as
    /// **`Empty`**: `CSI 20 t` and `CSI 21 t` are answered, but with an empty
    /// OSC string rather than with the title. That is a deliberate mitigation —
    /// a terminal that echoes its own title into the input stream lets anything
    /// that can write to the screen put text in front of the shell — and it is
    /// what this models when true. False is upstream's `ignore`, which answers
    /// nothing at all.
    ///
    /// The third mode, `accept`, reports the real title, and its four spellings
    /// interleave `ts.Title` from the INI with the remote one. That needs the
    /// settings surface, so it is not offered here rather than being guessed at.
    pub title_report: bool,
    /// `ts.AcceptTitleChangeRequest`, as the boolean the title stack uses it as
    /// (`vtterm.c:2758`). Upstream's default is `overwrite`, so: on. The full
    /// four-way enum is `TERATERM.INI`'s and belongs to Stage 2's schema.
    pub accept_title_change: bool,
    /// DECRQCRA, the rectangular-area checksum — **and the one thing here that
    /// is not upstream's**. Tera Term has no `CSI * y` at all; `vtterm.c` never
    /// mentions a checksum, so the faithful answer to the request is silence,
    /// and that is what the default gives.
    ///
    /// It exists because it is the only way to read a cell back over the wire,
    /// and `esctest/` — iTerm2's conformance suite, which runs *inside* the
    /// terminal — asserts on screen contents through nothing else. So the
    /// conformance harness turns it on and everything else leaves it off,
    /// which keeps a real connection byte-for-byte Tera Term while still
    /// letting a thousand assertions look at the grid.
    pub decrqcra: bool,
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
    /// `ts.MouseEventTracking` (`ttset.c:1523`, key default **on**). With it
    /// off, every `DECSET 9/1000…1016` is a no-op and DECRQM reports those
    /// modes as permanently reset.
    pub mouse_tracking_enabled: bool,
    /// `ts.DisableMouseTrackingByCtrl` (`ttset.c:1591`, key default on). Holding
    /// Ctrl suppresses the report so the user can still select text.
    pub disable_mouse_tracking_by_ctrl: bool,
    /// The character cell in pixels. The core needs it for exactly two things:
    /// converting a mouse position to a cell, and SGR-pixel reports, which do
    /// not convert at all. It never learns anything else about pixels.
    pub cell_w: i32,
    pub cell_h: i32,
    /// `ts.TranslateWheelToCursor` (`ttset.c:1515`, key default on). Gates
    /// `DECSET 7786`, and is what a reset restores that mode to.
    pub translate_wheel_to_cursor: bool,
    /// `WF_CURSORCHANGE` (`ttset.c:1656` `CursorCtrlSequence`, key default
    /// **off**). Gates DECSCUSR and `DECSET 12`, and shifts what DECRQM
    /// answers for the three cursor modes by two.
    pub cursor_ctrl_sequence: bool,
    /// `ts.LocalEcho` (`ttset.c:660`, key default off). SRM writes it.
    pub local_echo: bool,
    /// `ts.CRSend` (`ttset.c:657`, default `IdCR`). LNM overwrites it at
    /// runtime, so this is only the starting value.
    pub cr_send: CrSend,
    /// `ts.BSKey == IdBS` (`ttset.c:882`, the else branch). DECBKM writes it.
    pub bs_key_is_bs: bool,
    /// `ts.DisableAppKeypad` (`ttset.c:903`, key default off). Vetoes DECNKM
    /// for key encoding while leaving the mode itself set, which is what
    /// DECRQM keeps reporting.
    pub disable_app_keypad: bool,
    /// `ts.DisableAppCursor` (`ttset.c:907`, key default off). Same veto for
    /// DECCKM.
    pub disable_app_cursor: bool,
}

/// The private and ANSI modes that are one flag each. Grouped so a reset can
/// name them together, and so `DECRQM` has one place to read.
#[derive(Clone, Debug)]
struct Modes {
    /// DECCKM. The frontend needs it to pick `ESC O A` over `ESC [ A`.
    appli_cursor: bool,
    /// DECNKM.
    appli_key: bool,
    /// `AppliEscapeMode` — 0, or mintty's 1 from `DECSET 7727`, or 2/3/4 from
    /// the three upstream test modes at 14002-14004.
    appli_escape: u16,
    /// DECARM. On out of reset.
    auto_repeat: bool,
    /// DECTCEM, `IsCaretEnabled()`. On out of reset.
    caret: bool,
    /// DECPEX. Starts **set** — `vtterm.c:176` initialises it TRUE and
    /// nothing resets it, so DECRQM reports 1 on a fresh terminal.
    print_ex: bool,
    /// `DECSET 2004`.
    bracketed_paste: bool,
    /// `DECSET 7786`, `AcceptWheelToCursor`.
    wheel_to_cursor: bool,
    /// `DECSET 8200`. Makes `ED 2` home the cursor afterwards.
    clear_then_home: bool,
    /// DECSCNM. Upstream keeps it in `ts.ColorFlag`; nothing in the grid reads
    /// it, since reversing the whole screen is the renderer's job.
    reverse_video: bool,
    /// KAM. On out of reset — the keyboard is not locked.
    keyb_enabled: bool,
    /// SRM, mirroring `ts.LocalEcho`. Note the sense: `SM 12` turns local echo
    /// *off*.
    local_echo: bool,
    /// LNM. It also rewrites `cv.CRSend`, which is why the pair moves
    /// together — `SM 20` means CR LF, `RM 20` means bare CR, and neither
    /// leaves an `IdLF` from the config in place.
    lf_mode: bool,
    cr_send: CrSend,
    /// DECBKM, mirroring `ts.BSKey`.
    bs_key_is_bs: bool,
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
            title_report: true,
            accept_title_change: true,
            decrqcra: false,
            send_8bit_ctrl: false,
            cursor_shape: 1,
            nonblinking_cursor: false,
            // ttset.c:1213 MaxBuffSize. Not ttset.c:750's ScrollBuffSize (100),
            // which is the *initial* depth the user can grow up to this.
            scrollback_max: 10_000,
            mouse_tracking_enabled: true,
            disable_mouse_tracking_by_ctrl: true,
            // Matches the oracle's nominal cell (`oracle.h:ORACLE_CELL_W`), so
            // an injected event lands on the same character in both engines.
            cell_w: 8,
            cell_h: 16,
            translate_wheel_to_cursor: true,
            cursor_ctrl_sequence: false,
            local_echo: false,
            cr_send: CrSend::Cr,
            bs_key_is_bs: true,
            disable_app_keypad: false,
            disable_app_cursor: false,
        }
    }
}

impl Modes {
    /// `vtterm.c:ResetTerminal`. Note what is *not* here: DECPEX, the keyboard
    /// lock, local echo and DECBKM all survive a RIS, because upstream never
    /// clears them. Bracketed paste does not — it is cleared much further
    /// down the same function (`vtterm.c:336`), a hundred lines after the
    /// block that clears the other modes.
    fn reset(&mut self, config: &Config) -> Modes {
        Modes {
            appli_cursor: false,
            appli_key: false,
            appli_escape: 0,
            auto_repeat: true,
            caret: true,
            print_ex: self.print_ex,
            bracketed_paste: false,
            wheel_to_cursor: config.translate_wheel_to_cursor,
            clear_then_home: false,
            reverse_video: false,
            keyb_enabled: self.keyb_enabled,
            local_echo: self.local_echo,
            lf_mode: self.lf_mode,
            cr_send: self.cr_send,
            bs_key_is_bs: self.bs_key_is_bs,
        }
    }

    /// `vtterm.c:SoftReset`, which is a much shorter list.
    fn soft_reset(&mut self, config: &Config) {
        self.auto_repeat = true;
        self.caret = true;
        self.appli_cursor = false;
        self.appli_key = false;
        self.appli_escape = 0;
        self.wheel_to_cursor = config.translate_wheel_to_cursor;
    }

    fn from_config(config: &Config) -> Modes {
        Modes {
            appli_cursor: false,
            appli_key: false,
            appli_escape: 0,
            auto_repeat: true,
            caret: true,
            print_ex: true,
            bracketed_paste: false,
            wheel_to_cursor: config.translate_wheel_to_cursor,
            clear_then_home: false,
            reverse_video: false,
            keyb_enabled: true,
            local_echo: config.local_echo,
            lf_mode: config.cr_send == CrSend::CrLf,
            cr_send: config.cr_send,
            bs_key_is_bs: config.bs_key_is_bs,
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
    /// Continuation bytes still owed by a UTF-8 sequence in progress, across
    /// chunk boundaries for the same reason. A byte in `80..=9F` is a C1
    /// control only when nothing is expecting it.
    utf8_left: u8,
    /// The start of that sequence, held back rather than handed to `vte`.
    ///
    /// **`vte` 0.15.0 loses bytes when it resumes one.**
    /// `advance_partial_utf8` (`lib.rs:687`) fills a 4-byte buffer, decodes it,
    /// prints only the *first* character — "we just ignore the rest", says the
    /// comment — and then returns `valid_up_to()` as the number of bytes it
    /// consumed. Any complete character between that first one and the
    /// incomplete tail is dropped without a trace. Reachable whenever a 2-byte
    /// sequence is cut by a read boundary and the next read holds exactly one
    /// ASCII byte before another multi-byte lead: `[C3 A9]` split as
    /// `[.. C3] [A9 'a' E4 B8 80]` prints `é一` and eats the `a`.
    ///
    /// So the partial sequence never reaches `vte` at all — it waits here and
    /// goes out with the next chunk. That is the right place for it regardless:
    /// this walk already has to know where sequences begin and end, because
    /// telling a continuation byte from a bare C1 control is the whole reason
    /// [`Vt::rewrite_c1`] exists.
    held: Vec<u8>,
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
                modes: Modes::from_config(&config),
                config,
                vt_level,
                send_8bit,
                ..State::empty()
            },
            pending_c2: false,
            utf8_left: 0,
            held: Vec::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        // Pure ASCII needs no rewriting at all, and almost every chunk is. The
        // test cannot be narrower than this: the walk has to see whole UTF-8
        // sequences to know a continuation byte from a bare one, so any byte
        // over 0x7F puts it back in play.
        if !self.pending_c2 && self.utf8_left == 0 && !bytes.iter().any(|&b| b >= 0x80) {
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
    ///
    /// The other half of the job is a byte in `80..=9F` that is **not** part of
    /// any sequence. `vte` executes it as a C1 control; Tera Term's decoder
    /// never sees a control at all, because a lone continuation byte is invalid
    /// UTF-8 and comes out as U+FFFD. Telling the two apart is the whole reason
    /// this walk has to track sequence lengths rather than look at bytes one at
    /// a time — the `80` in an em dash's `E2 80 94` is a continuation byte, and
    /// replacing it would eat the character.
    fn rewrite_c1(&mut self, bytes: &[u8]) -> Vec<u8> {
        let accept = self.state.config.accept_8bit_ctrl;
        let level = self.state.vt_level;
        // Whatever was held back last time leads off, already vetted. If it is
        // there at all then `utf8_left` is non-zero, so the loop below picks up
        // mid-sequence exactly where it left off.
        let mut out = std::mem::take(&mut self.held);
        out.reserve(bytes.len());
        // Where the sequence in progress starts in `out`. Only meaningful while
        // `utf8_left > 0`, and 0 on entry because that is where `held` put it.
        let mut seq_start = 0;

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
            } else if self.utf8_left > 0 {
                if (0x80..=0xbf).contains(&b) {
                    self.utf8_left -= 1;
                    out.push(b);
                    continue;
                }
                // The sequence was cut short. Whatever this byte is, it starts
                // something new rather than continuing what came before.
                self.utf8_left = 0;
            }

            match b {
                // The only lead byte that can produce a C1 codepoint, and so
                // the only one worth holding back.
                0xc2 => self.pending_c2 = true,
                0xc0..=0xdf => {
                    self.utf8_left = 1;
                    seq_start = out.len();
                    out.push(b);
                }
                0xe0..=0xef => {
                    self.utf8_left = 2;
                    seq_start = out.len();
                    out.push(b);
                }
                0xf0..=0xf7 => {
                    self.utf8_left = 3;
                    seq_start = out.len();
                    out.push(b);
                }
                // A bare C1 byte. Invisible until a line is not 8-bit clean or
                // the baud rate is wrong, and then it is the whole difference
                // between a screen full of replacement characters — which says
                // *something is arriving and it is wrong* — and a screen that
                // stays blank, which says the far end is dead.
                0x80..=0x9f => out.extend_from_slice("\u{fffd}".as_bytes()),
                _ => out.push(b),
            }
        }

        // A sequence still in progress goes no further. See [`Vt::held`] — the
        // point is that `vte` never sees a partial one, because its resumption
        // path drops bytes.
        if self.utf8_left > 0 {
            self.held = out.split_off(seq_start);
        }
        out
    }

    pub fn grid(&self) -> &Grid {
        &self.state.grid
    }

    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.state.grid
    }

    /// The settings the terminal is running under.
    ///
    /// A settings dialog starts from this rather than from the defaults, so
    /// that a page nobody opened does not quietly reset what it holds — and so
    /// that the four fields an escape sequence can write are read back as they
    /// now are, not as the file left them.
    pub fn config(&self) -> &Config {
        &self.state.config
    }

    /// Apply settings to a running terminal — the dialog's OK button, which
    /// upstream spells `CVTWindow::SetupTerm` (`vtwin.cpp:1383`).
    ///
    /// **Upstream keeps no separate copy of most of this.** `vtterm.c` reads
    /// `ts` at the point of use, so writing the setting *is* applying it, and
    /// the escape sequences write the very same variables back: DECBKM assigns
    /// `ts.BSKey` (`vtterm.c:2992`), SRM assigns `ts.LocalEcho` (`:2053`), LNM
    /// assigns `ts.CRSend` (`:2059`). Our `Modes` holds copies of those
    /// three, so they are refreshed here — which means applying settings
    /// overwrites what the host asked for, exactly as it does upstream. That is
    /// surprising the first time it happens and is not a bug.
    ///
    /// The two upstream *does* keep separately are deliberately left alone:
    /// `LFMode` and `AcceptWheelToCursor` are seeded from `ts` at reset
    /// (`vtterm.c:285`, `:290`) and `SetupTerm` never touches them.
    ///
    /// The grid is resized to match, so a caller with a connection has to tell
    /// the far end afterwards — `tt_session::Session::apply_settings` is the
    /// thing that does both.
    pub fn set_config(&mut self, config: Config) {
        let s = &mut self.state;
        s.config = config;

        // `ChangeTerminalID` (`vtterm.c:5850`): a terminal ID changed in the
        // dialog re-derives the VT level, and level 1 never sends 8-bit
        // controls whatever the setting says.
        s.vt_level = s.config.term_id.vt_level();
        s.send_8bit = s.vt_level >= 2 && s.config.send_8bit_ctrl;

        s.modes.local_echo = s.config.local_echo;
        s.modes.bs_key_is_bs = s.config.bs_key_is_bs;
        s.modes.cr_send = s.config.cr_send;

        s.grid.set_scrollback_max(s.config.scrollback_max);
        s.grid.resize(s.config.cols, s.config.rows);

        // `SetupTerm` opens with `ResetCharSet()`, so a G1 designation made by
        // the host does not survive the dialog. Reproduced rather than
        // improved on: it is the same rule as the three above.
        s.charset.reset();
    }

    /// Bytes the terminal wants to send back to the host: DA, DSR, and friends.
    pub fn reply(&self) -> &[u8] {
        &self.state.reply
    }

    pub fn take_reply(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.state.reply)
    }

    /// Append bytes to the host-bound stream, for input the terminal did not
    /// generate itself — a key, or a paste. Upstream's keyboard writes
    /// through the same `CommBinaryOut` the escape replies use, so keeping
    /// them in one stream is what makes the two engines comparable.
    pub fn push_reply(&mut self, bytes: &[u8]) {
        self.state.send(bytes);
    }

    /// The last title set by OSC 0 / OSC 2. Empty if never set.
    pub fn title(&self) -> &str {
        &self.state.title
    }

    /// Report a mouse event, in **window pixels**. Returns true if the
    /// terminal consumed it — the frontend should then not also treat the
    /// click as the start of a text selection.
    ///
    /// `button` is upstream's numbering ([`mouse::BUTTON_LEFT`] and friends);
    /// for [`MouseEvent::Wheel`] it is 0 for up and 1 for down.
    pub fn mouse_event(
        &mut self,
        event: MouseEvent,
        button: u8,
        px: i32,
        py: i32,
        mods: Modifiers,
    ) -> bool {
        self.state.mouse_report(event, button, px, py, mods)
    }

    /// Focus gained or lost. Emits nothing unless `DECSET 1004` is on.
    pub fn focus_event(&mut self, focused: bool) {
        self.state.focus_report(focused);
    }

    /// Which mouse events the host has asked for. The frontend needs this to
    /// decide whether to track motion at all.
    pub fn mouse_tracking(&self) -> Tracking {
        self.state.mouse.tracking
    }

    pub fn mouse_encoding(&self) -> Encoding {
        self.state.mouse.encoding
    }

    pub fn focus_reporting(&self) -> bool {
        self.state.mouse.focus_report
    }

    /// Tell the core how big a character cell is, after a font change. It is
    /// used only to convert mouse positions and to answer pixel-mode reports.
    pub fn set_cell_pixels(&mut self, w: i32, h: i32) {
        self.state.config.cell_w = w.max(1);
        self.state.config.cell_h = h.max(1);
    }

    /// DECCKM. The frontend sends `ESC O A` rather than `ESC [ A` while it is
    /// on — but the keymap lives in the core, so this exists for the shell's
    /// own decisions (scrollbar behaviour, wheel translation) rather than for
    /// building key sequences.
    pub fn application_cursor_keys(&self) -> bool {
        self.state.modes.appli_cursor
    }

    /// DECNKM.
    pub fn application_keypad(&self) -> bool {
        self.state.modes.appli_key
    }

    /// DECTCEM. False means the cursor should not be drawn.
    pub fn cursor_visible(&self) -> bool {
        self.state.modes.caret
    }

    /// `DECSET 2004`. A paste must be wrapped in `ESC [ 200 ~` … `ESC [ 201 ~`
    /// while it is on.
    pub fn bracketed_paste(&self) -> bool {
        self.state.modes.bracketed_paste
    }

    /// DECSCNM. The whole screen is drawn with foreground and background
    /// swapped; nothing in the grid changes, which is why it lives here.
    pub fn reverse_video(&self) -> bool {
        self.state.modes.reverse_video
    }

    /// KAM. The host has asked that typing be ignored.
    pub fn keyboard_enabled(&self) -> bool {
        self.state.modes.keyb_enabled
    }

    /// SRM — the host wants the terminal to echo locally.
    pub fn local_echo(&self) -> bool {
        self.state.modes.local_echo
    }

    /// LNM. A CR from the keyboard sends CR LF while it is on.
    pub fn newline_mode(&self) -> bool {
        self.state.modes.lf_mode
    }

    /// DECBKM. False means Backspace sends DEL rather than BS.
    pub fn backspace_sends_bs(&self) -> bool {
        self.state.modes.bs_key_is_bs
    }

    /// `DECSET 7786` — translate the wheel into cursor keys when the
    /// application cursor mode is on.
    pub fn wheel_to_cursor(&self) -> bool {
        self.state.modes.wheel_to_cursor
    }

    /// The modes [`Key::encode`] reads, as the terminal currently stands.
    ///
    /// The two `Disable*` settings veto their mode *here* rather than at
    /// DECSET time, matching `keyboard.c:KeyCodeSend` — so a host can set
    /// DECCKM, have DECRQM confirm it, and still get the normal cursor keys.
    pub fn key_modes(&self) -> KeyModes {
        let m = &self.state.modes;
        KeyModes {
            application_cursor: m.appli_cursor && !self.state.config.disable_app_cursor,
            application_keypad: m.appli_key && !self.state.config.disable_app_keypad,
            eight_bit: self.state.send_8bit,
            cr_send: m.cr_send,
        }
    }

    /// What `key` puts on the wire right now, or `None` for a key that is a
    /// local command. The core owns this because the encoding depends on
    /// terminal state the frontend never sees.
    pub fn key(&self, key: Key) -> Option<Vec<u8>> {
        key.encode(self.key_modes())
    }

    /// Turn the plain-text log tap on or off.
    ///
    /// While it is on, every character the parser decides to *print* is
    /// recorded, plus a newline for each line feed. Escape sequences never
    /// appear because the parser has already consumed them — which is the
    /// point, and the reason this lives here rather than in a stripper beside
    /// the log. A second escape-sequence scanner would be one more place to
    /// disagree with the one that is verified against Tera Term.
    ///
    /// This is upstream's `FLogPutUTF32` seam, reached from `vtterm.c:468`
    /// with the same characters at the same moments.
    pub fn set_log_text_enabled(&mut self, on: bool) {
        if on {
            self.state.log_text.get_or_insert_with(String::new);
        } else {
            self.state.log_text = None;
        }
    }

    /// Take what the tap has collected since the last call, leaving the buffer
    /// allocated for reuse. Empty when the tap is off.
    pub fn take_log_text(&mut self) -> String {
        match &mut self.state.log_text {
            Some(buf) => std::mem::take(buf),
            None => String::new(),
        }
    }

    /// Encode typed text — `ttcmn.c:OutControl`, which every `IdText` byte
    /// goes through on its way out.
    ///
    /// The only thing it does today is expand CR by [`CrSend`], and that one
    /// thing is why the function exists: the main Return key is **not** in
    /// [`Key`], because upstream handles `VK_RETURN` in `KeyDown` rather than
    /// in the key table, marking it `IdText` precisely so this conversion
    /// applies (`keyboard.c:908`). A frontend sending a bare `\r` would
    /// otherwise have to know about LNM, and the keymap is the core's.
    ///
    /// Not applied to a paste: bracketed paste means "send this verbatim",
    /// and rewriting newlines inside pasted text is how a heredoc or a YAML
    /// block arrives corrupted.
    ///
    /// Telnet adds an arm here — `IdCR` with the connection not in binary
    /// mode appends a NUL — which is `cv->TelFlag` and belongs with the
    /// transport that has it.
    pub fn encode_text(&self, text: &str) -> Vec<u8> {
        let cr_send = self.state.modes.cr_send;
        if cr_send == CrSend::Cr || !text.contains('\r') {
            return text.as_bytes().to_vec();
        }
        let mut out = Vec::with_capacity(text.len() + 8);
        for b in text.bytes() {
            match (b, cr_send) {
                (0x0d, CrSend::CrLf) => out.extend_from_slice(&[0x0d, 0x0a]),
                (0x0d, CrSend::Lf) => out.push(0x0a),
                _ => out.push(b),
            }
        }
        out
    }
}

struct State {
    grid: Grid,
    config: Config,
    charset: Iso2022,
    /// DECSC saves the G-sets alongside the cursor — `vtterm.c:228` — and so
    /// there is one slot per screen, the same two `Grid` keeps the position in.
    saved_charset: [Option<Iso2022State>; 2],
    alt_screen: bool,
    reply: Vec<u8>,
    title: String,
    /// `TitleStack` (`vtterm.c:2757`) — what `CSI 22 t` puts away and
    /// `CSI 23 t` brings back.
    title_stack: Vec<String>,
    /// `ts.CRReceive == Auto` keeps one byte of history to collapse CR+LF.
    prev_was_cr: bool,
    prev_was_lf: bool,
    auto_generated_crlf: bool,
    /// The last printable codepoint, for REP.
    last_printed: Option<u32>,
    /// The plain-text log tap — `Some` only while a text log is open, so the
    /// cost when it is not is one branch per printed character. See
    /// [`Vt::take_log_text`].
    log_text: Option<String>,
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
    mouse: mouse::MouseState,
    modes: Modes,
}

impl State {
    fn empty() -> Self {
        State {
            grid: Grid::new(1, 1, 0),
            config: Config::default(),
            charset: Iso2022::new(),
            saved_charset: [None, None],
            alt_screen: false,
            reply: Vec::new(),
            title: String::new(),
            title_stack: Vec::new(),
            prev_was_cr: false,
            prev_was_lf: false,
            auto_generated_crlf: false,
            last_printed: None,
            log_text: None,
            rect_mode: false,
            lr_margin_mode: false,
            vt_level: 1,
            send_8bit: false,
            dcs: None,
            dcs_buf: Vec::new(),
            mouse: mouse::MouseState::default(),
            modes: Modes::from_config(&Config::default()),
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
        self.send_csi_bytes(body.as_bytes());
    }

    /// The mouse formats are not all UTF-8, so they arrive as raw bytes.
    fn send_csi_bytes(&mut self, body: &[u8]) {
        if self.send_8bit {
            self.send(&[0x9b]);
        } else {
            self.send(b"\x1b[");
        }
        self.send(body);
    }

    /// `vtterm.c:MouseReport`. Returns whether the event was consumed by
    /// reporting — the frontend uses that to decide whether the click is also
    /// a text selection.
    ///
    /// `px`/`py` are window pixels. Every branch below is upstream's, including
    /// the ones that return `true` after sending nothing.
    fn mouse_report(
        &mut self,
        event: MouseEvent,
        button: u8,
        px: i32,
        py: i32,
        m: Modifiers,
    ) -> bool {
        let button = button as i32;

        // The button mask and the last position are updated before any of the
        // early exits, so they stay live while reporting is off. DECRQLP can
        // read them back afterwards.
        match event {
            MouseEvent::Press => self.mouse.button_stat |= 8 >> (button + 1),
            MouseEvent::Release => self.mouse.button_stat &= !(8 >> (button + 1)),
            _ => {}
        }
        self.mouse.last = (px, py);

        if self.mouse.tracking == Tracking::None {
            return false;
        }
        if self.config.disable_mouse_tracking_by_ctrl && m.ctrl {
            return false;
        }
        if self.mouse.tracking == Tracking::DecLocator {
            return self.locator_report(event, button);
        }

        let (x, y) = if self.mouse.encoding == Encoding::SgrPixels {
            // Pixels go out unconverted; only the floor at 1 applies.
            (px.max(1), py.max(1))
        } else {
            let (cx, cy) = self.win_to_screen(px, py);
            (
                (cx + 1).clamp(1, self.grid.cols() as i32),
                (cy + 1).clamp(1, self.grid.rows() as i32),
            )
        };
        let modifier = m.bits();

        let body = match event {
            MouseEvent::CurStat => return false,
            MouseEvent::Press => match self.mouse.tracking {
                Tracking::X10 => mouse::encode(self.mouse.encoding, button, x, y),
                Tracking::Vt200 | Tracking::BtnEvent | Tracking::AllEvent => {
                    self.mouse.last_send = (x, y);
                    self.mouse.last_button = button;
                    mouse::encode(self.mouse.encoding, button | modifier, x, y)
                }
                Tracking::NetTerm => {
                    // Not a CSI, and not routed through SendCSIstr — upstream
                    // writes it to the wire directly (`vtterm.c:5687`).
                    let raw = format!("\x1b}}{y},{x}\r");
                    self.send(raw.as_bytes());
                    return true;
                }
                _ => return false,
            },
            MouseEvent::Release => match self.mouse.tracking {
                Tracking::Vt200 | Tracking::BtnEvent | Tracking::AllEvent => {
                    // The SGR forms can say *which* button was released; the
                    // others cannot, and report the anonymous button 3.
                    let mb = if matches!(self.mouse.encoding, Encoding::Sgr | Encoding::SgrPixels) {
                        button | modifier | 128
                    } else {
                        mouse::BUTTON_RELEASE as i32 | modifier
                    };
                    self.mouse.last_send = (x, y);
                    self.mouse.last_button = mouse::BUTTON_RELEASE as i32;
                    mouse::encode(self.mouse.encoding, mb, x, y)
                }
                // Nothing to send, but the event is still consumed.
                Tracking::X10 | Tracking::NetTerm => return true,
                _ => return false,
            },
            MouseEvent::Move => match self.mouse.tracking {
                // 1002 reports motion only while a button is held, and the
                // test is against the last button *sent*, not the mask.
                Tracking::BtnEvent | Tracking::AllEvent => {
                    if self.mouse.tracking == Tracking::BtnEvent
                        && self.mouse.last_button == mouse::BUTTON_RELEASE as i32
                    {
                        return false;
                    }
                    if (x, y) == self.mouse.last_send {
                        return false;
                    }
                    self.mouse.last_send = (x, y);
                    let mb = self.mouse.last_button | modifier | 32;
                    mouse::encode(self.mouse.encoding, mb, x, y)
                }
                _ => return false,
            },
            MouseEvent::Wheel => match self.mouse.tracking {
                Tracking::Vt200 | Tracking::BtnEvent | Tracking::AllEvent => {
                    mouse::encode(self.mouse.encoding, button | modifier | 64, x, y)
                }
                _ => return false,
            },
        };

        if body.is_empty() {
            return false;
        }
        self.send_csi_bytes(&body);
        true
    }

    /// `vtterm.c:DecLocatorReport`.
    fn locator_report(&mut self, event: MouseEvent, button: i32) -> bool {
        let (last_x, last_y) = self.mouse.last;
        let pixel = self.mouse.locator_flags & mouse::PIXEL != 0;

        // Out of range is signalled by a negative x alone; y keeps its value
        // and is simply not printed.
        let (mut x, y) = if pixel {
            let (max_x, max_y) =
                self.screen_to_win(self.grid.cols() as i32 + 1, self.grid.rows() as i32 + 1);
            let (x, y) = (last_x + 1, last_y + 1);
            (
                if x < 1 || x > max_x || y < 1 || y > max_y {
                    -1
                } else {
                    x
                },
                y,
            )
        } else {
            let (cx, cy) = self.win_to_screen(last_x, last_y);
            let (x, y) = (cx + 1, cy + 1);
            let ok =
                x >= 1 && x <= self.grid.cols() as i32 && y >= 1 && y <= self.grid.rows() as i32;
            (if ok { x } else { -1 }, y)
        };

        let stat = self.mouse.button_stat;
        let body = match event {
            MouseEvent::CurStat => {
                if self.mouse.tracking == Tracking::DecLocator {
                    mouse::encode_locator(1, stat, x, y)
                } else {
                    // DECRQLP with no locator enabled still answers, to say so.
                    b"0&w".to_vec()
                }
            }
            MouseEvent::Press if self.mouse.locator_flags & mouse::BUTTON_DOWN != 0 => {
                mouse::encode_locator(button * 2 + 2, stat, x, y)
            }
            MouseEvent::Release if self.mouse.locator_flags & mouse::BUTTON_UP != 0 => {
                mouse::encode_locator(button * 2 + 3, stat, x, y)
            }
            MouseEvent::Move if self.mouse.locator_flags & mouse::FILTERED != 0 => {
                let (top, left, bottom, right) = self.mouse.filter;
                if y < top || y > bottom || x < left || x > right {
                    self.mouse.locator_flags &= !mouse::FILTERED;
                    // The filter compares against the *unclamped* x, so an
                    // out-of-page locator escapes the rectangle by definition.
                    x = if x < 1 { -1 } else { x };
                    mouse::encode_locator(10, stat, x, y)
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };

        if body.is_empty() {
            return false;
        }
        self.send_csi_bytes(&body);
        if self.mouse.locator_flags & mouse::ONE_SHOT != 0 {
            self.mouse.tracking = Tracking::None;
        }
        true
    }

    /// `vtdisp.c:DispConvWinToScreen`, with `WinOrgX`/`WinOrgY` fixed at zero —
    /// the core has no scrolled-back viewport of its own.
    fn win_to_screen(&self, px: i32, py: i32) -> (i32, i32) {
        (
            px / self.config.cell_w.max(1),
            py / self.config.cell_h.max(1),
        )
    }

    fn screen_to_win(&self, cx: i32, cy: i32) -> (i32, i32) {
        (cx * self.config.cell_w, cy * self.config.cell_h)
    }

    /// `vtterm.c:FocusReport`.
    fn focus_report(&mut self, focused: bool) {
        if self.mouse.focus_report {
            self.send_csi(if focused { "I" } else { "O" });
        }
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

    /// `vtterm.c:AnswerTerminalType` — Primary DA, and DECID (`ESC Z`), which
    /// upstream answers with the same function.
    ///
    /// It does *not* use `SendCSIstr`. It writes its own introducer, and gates
    /// the 8-bit form on `ts.TerminalID >= IdVT320` as well as on
    /// `Send8BitMode` — so a VT220 told S8C1T answers DSR with `9B` and this
    /// with `ESC [`. Note the test is the terminal *id*, not the VT level, so
    /// DECSCL cannot change it.
    fn primary_da(&mut self) {
        let da = self.config.term_id.primary_da();
        if self.send_8bit && self.config.term_id.ordinal() >= TermId::Vt320.ordinal() {
            self.send(&[0x9b, b'?']);
        } else {
            self.send(b"\x1b[?");
        }
        self.send(da.as_bytes());
        self.send(b"c");
    }

    /// `vtterm.c:LineFeed` — the whole of it, which is what IND and NEL call
    /// as well as the LF byte.
    ///
    /// The tail is the one that surprises: with LNM set, a line feed **returns
    /// the carriage too**, and that is the *receive* side of a mode everything
    /// else treats as being about what the keyboard sends. Upstream does it
    /// after the vertical move, not before (`vtterm.c:706`).
    fn line_feed(&mut self) {
        self.grid.line_feed();
        if self.modes.lf_mode {
            self.grid.carriage_return();
        }
    }

    /// `vtterm.c:747`.
    fn process_lf(&mut self) {
        match self.config.cr_receive {
            CrReceive::Lf => {
                // "the server sends LF alone" — so LF means CR+LF.
                self.grid.carriage_return();
                self.line_feed();
            }
            CrReceive::Auto => {
                if !self.prev_was_cr || !self.auto_generated_crlf {
                    self.grid.carriage_return();
                    self.line_feed();
                    self.auto_generated_crlf = true;
                } else {
                    self.auto_generated_crlf = false;
                }
            }
            _ => self.line_feed(),
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

                    // Mouse tracking. Every *set* is gated on the setting;
                    // every *reset* is unconditional, and resets the whole
                    // mode rather than only the one named — so `DECRESET 9`
                    // turns off any-event tracking too (`vtterm.c:3135`).
                    9 | 1000 | 1001 | 1002 | 1003 | 14001 => {
                        if !on {
                            self.mouse.tracking = Tracking::None;
                        } else if self.config.mouse_tracking_enabled {
                            self.mouse.tracking = match p {
                                9 => Tracking::X10,
                                1000 => Tracking::Vt200,
                                1001 => Tracking::Vt200Hl,
                                1002 => Tracking::BtnEvent,
                                1003 => Tracking::AllEvent,
                                _ => Tracking::NetTerm,
                            };
                        }
                    }
                    1004 => {
                        if !on {
                            self.mouse.focus_report = false;
                        } else if self.config.mouse_tracking_enabled {
                            self.mouse.focus_report = true;
                        }
                    }
                    1005 | 1006 | 1015 | 1016 => {
                        if !on {
                            self.mouse.encoding = Encoding::Normal;
                        } else if self.config.mouse_tracking_enabled {
                            self.mouse.encoding = match p {
                                1005 => Encoding::Utf8,
                                1006 => Encoding::Sgr,
                                1015 => Encoding::Urxvt,
                                _ => Encoding::SgrPixels,
                            };
                        }
                    }
                    // The rest are one flag each, and are here rather than
                    // dropped because the frontend reads several of them and
                    // DECRQM reports all of them.
                    1 => self.modes.appli_cursor = on,
                    3 => self.dec_colm(on),
                    5 => self.modes.reverse_video = on,
                    8 => self.modes.auto_repeat = on,
                    // `DECSET 12` and DECSCUSR are both gated on a setting
                    // that ships *off*, so by default this does nothing.
                    12 => {
                        if self.config.cursor_ctrl_sequence {
                            self.config.nonblinking_cursor = !on;
                        }
                    }
                    19 => self.modes.print_ex = on,
                    25 => self.modes.caret = on,
                    66 => self.modes.appli_key = on,
                    67 => self.modes.bs_key_is_bs = on,
                    2004 => self.modes.bracketed_paste = on,
                    7727 => self.modes.appli_escape = u16::from(on),
                    7786 => {
                        if !on {
                            self.modes.wheel_to_cursor = false;
                        } else if self.config.translate_wheel_to_cursor {
                            self.modes.wheel_to_cursor = true;
                        }
                    }
                    8200 => self.modes.clear_then_home = on,
                    14002..=14004 => {
                        self.modes.appli_escape = if on { p - 14000 } else { 0 };
                    }
                    _ => {}
                }
            } else {
                match p {
                    // KAM. `SM 2` *locks* the keyboard, so the sense inverts.
                    2 => self.modes.keyb_enabled = !on,
                    4 => self.grid.insert_mode = on,
                    // SRM inverts too: `SM 12` means "send/receive", which is
                    // local echo off.
                    12 => self.modes.local_echo = !on,
                    20 => {
                        self.modes.lf_mode = on;
                        self.modes.cr_send = if on { CrSend::CrLf } else { CrSend::Cr };
                    }
                    _ => {}
                }
            }
        }
    }

    /// DECCOLM — `vtterm.c:CSQChangeColumnMode`. It is a resize, it throws the
    /// left/right margins away, and because `TF_CLEARONRESIZE` ships off it
    /// also clears the screen and homes the cursor by hand.
    fn dec_colm(&mut self, wide: bool) {
        let rows = self.grid.rows();
        self.grid.resize(if wide { 132 } else { 80 }, rows);
        self.lr_margin_mode = false;
        self.grid.reset_lr_margins();
        self.grid.move_cursor(0, 0);
        self.grid.clear_screen();
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

    /// DECRQCRA — `CSI Pid ; Pp ; Pt ; Pl ; Pb ; Pr * y`, answered
    /// `DCS Pid ! ~ HHHH ST`.
    ///
    /// **Nothing upstream corresponds to this**; see [`Config::decrqcra`] for
    /// why it exists and why it is off by default. Three decisions it makes on
    /// its own, none of which upstream can arbitrate:
    ///
    /// - **The sum is over characters only, not attributes.** DEC STD 070 folds
    ///   the attribute bits in and xterm has resources to choose; `esctest`
    ///   asserts that a single cell's checksum *equals its character code*, so
    ///   anything added here would make every screen assertion in the suite
    ///   fail. A combining mark adds its own codepoint, and the padding half of
    ///   a wide character holds no codepoint and so adds nothing.
    /// - **It is the plain sum, not the two's complement.** That is xterm from
    ///   patch #279 onward; the harness passes `--xterm-checksum` high enough
    ///   to say so.
    /// - **An erased cell counts as a space**, because that is what an erase
    ///   leaves in the grid — Tera Term has no "empty versus blank"
    ///   distinction to preserve, which is xterm's behaviour from patch #334.
    ///
    /// An inverted rectangle answers zero rather than staying silent. The
    /// rectangular *operations* return without acting on one, which is
    /// upstream's behaviour and fine for something with no reply; doing the
    /// same to a request would leave the far end waiting on an answer that
    /// never comes.
    fn decrqcra(&mut self, params: &Params) {
        if !self.config.decrqcra {
            return;
        }
        let id = arg0(params, 0);
        let sum = match self.area_rect(params, 2) {
            Some(area) => {
                let mut sum: u16 = 0;
                for y in area.y0..=area.y1 {
                    for cell in &self.grid.line(y)[area.x0..=area.x1] {
                        for cp in cell.codepoints() {
                            sum = sum.wrapping_add(cp as u16);
                        }
                    }
                }
                sum
            }
            None => 0,
        };
        self.send_dcs(&format!("{id}!~{sum:04X}"));
    }

    /// `vtterm.c:SoftReset` — DECSTR (`CSI ! p`), and the first half of
    /// DECSCL.
    fn soft_reset(&mut self) {
        self.grid.soft_reset();
        self.charset.reset();
        self.saved_charset[usize::from(self.alt_screen)] = Some(self.charset.save());
        self.modes.soft_reset(&self.config);
    }

    /// `vtterm.c:SendDCSstr` — `ESC P … ESC \`, or the 8-bit `DCS … ST` when
    /// the terminal has been told it may.
    /// `vtterm.c:SendOSCstr` — `ESC ] … ST`, terminated with ST rather than
    /// BEL because that is what the title reports pass.
    fn send_osc(&mut self, body: &str) {
        if self.send_8bit {
            self.send(&[0x9d]);
            self.send(body.as_bytes());
            self.send(&[0x9c]);
        } else {
            self.send(b"\x1b]");
            self.send(body.as_bytes());
            self.send(b"\x1b\\");
        }
    }

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
            // Report icon label and window title. Gated on the *title* setting,
            // not on `WF_WINDOWREPORT`, and answered empty — see
            // `Config::title_report`.
            20 if self.config.title_report => self.send_osc("L"),
            21 if self.config.title_report => self.send_osc("l"),
            // Push and pop the title — `vtterm.c:2751`. The parameter names
            // which of icon and window title to stack, and all three values
            // do the same thing upstream because there is only one title.
            22 if self.config.accept_title_change => {
                if matches!(arg0(params, 1), 0..=2) {
                    let title = self.title.clone();
                    self.title_stack.push(title);
                }
            }
            23 if self.config.accept_title_change => {
                if matches!(arg0(params, 1), 0..=2) {
                    if let Some(title) = self.title_stack.pop() {
                        self.title = title;
                    }
                }
            }
            _ => {}
        }
    }

    /// DECRQM — `vtterm.c:CSDolRequestMode`. Answers `CSI [?]Ps;Ps $ y`, where
    /// the second parameter is 1 set, 2 reset, 3 permanently set, 4 permanently
    /// reset, and 0 "not recognised".
    ///
    /// The two halves differ in their fallback and that is upstream's, not a
    /// slip: an unknown **ANSI** mode answers 4, an unknown **DEC private**
    /// mode answers 0.
    fn decrqm(&mut self, mode: u16, private: bool) {
        let onoff = |b: bool| if b { 1 } else { 2 };
        let m = &self.modes;

        let resp: u16 = if private {
            // `!ts.MouseEventTracking` makes every mouse mode permanently
            // reset rather than merely off.
            let mouse = |live: bool| {
                if !self.config.mouse_tracking_enabled {
                    4
                } else {
                    onoff(live)
                }
            };
            // Without TF_ALTSCR the alternate screen is permanently reset —
            // except 1048, the cursor half, which is permanently *set*.
            let alt = |live: bool| {
                if !self.config.alt_screen_enabled {
                    4
                } else {
                    onoff(live)
                }
            };
            // The three cursor modes add two to their answer when the setting
            // that gates them is off, turning "set/reset" into "permanently
            // set/permanently reset".
            let cursor = |v: u16| {
                v + if self.config.cursor_ctrl_sequence {
                    0
                } else {
                    2
                }
            };

            match mode {
                1 => onoff(m.appli_cursor),
                3 => onoff(self.grid.cols() == 132),
                5 => onoff(m.reverse_video),
                6 => onoff(self.grid.origin_mode),
                7 => onoff(self.grid.autowrap),
                8 => onoff(m.auto_repeat),
                9 => mouse(self.mouse.tracking == Tracking::X10),
                12 => cursor(onoff(!self.config.nonblinking_cursor)),
                19 => onoff(m.print_ex),
                25 => onoff(m.caret),
                // DECTEK is permanently reset: the oracle has no Tek window
                // and neither will we.
                38 => 4,
                47 => alt(self.alt_screen),
                // DECKKDM answers 0 on a terminal that is not Japanese, which
                // this one never is.
                59 => 0,
                66 => onoff(m.appli_key),
                67 => onoff(m.bs_key_is_bs),
                69 => onoff(self.lr_margin_mode),
                1000 => mouse(self.mouse.tracking == Tracking::Vt200),
                // Highlight tracking is `#if 0`'d out upstream and always
                // answers permanently reset, even though setting it works.
                1001 => 4,
                1002 => mouse(self.mouse.tracking == Tracking::BtnEvent),
                1003 => mouse(self.mouse.tracking == Tracking::AllEvent),
                1004 => mouse(self.mouse.focus_report),
                1005 => mouse(self.mouse.encoding == Encoding::Utf8),
                1006 => mouse(self.mouse.encoding == Encoding::Sgr),
                1015 => mouse(self.mouse.encoding == Encoding::Urxvt),
                1016 => mouse(self.mouse.encoding == Encoding::SgrPixels),
                1047 => alt(self.alt_screen),
                1048 => {
                    if self.config.alt_screen_enabled {
                        1
                    } else {
                        4
                    }
                }
                1049 => alt(self.alt_screen),
                2004 => onoff(m.bracketed_paste),
                7727 => onoff(m.appli_escape == 1),
                7786 => {
                    if !self.config.translate_wheel_to_cursor {
                        4
                    } else {
                        onoff(m.wheel_to_cursor)
                    }
                }
                8200 => onoff(m.clear_then_home),
                14001 => mouse(self.mouse.tracking == Tracking::NetTerm),
                14002..=14004 => onoff(m.appli_escape == mode - 14000),
                _ => 0,
            }
        } else {
            let cursor = |v: u16| {
                v + if self.config.cursor_ctrl_sequence {
                    0
                } else {
                    2
                }
            };
            match mode {
                2 => onoff(!m.keyb_enabled),
                4 => onoff(self.grid.insert_mode),
                12 => onoff(!m.local_echo),
                20 => onoff(m.lf_mode),
                33 => cursor(onoff(self.config.nonblinking_cursor)),
                34 => cursor(onoff(self.config.cursor_shape == 3)),
                _ => 4,
            }
        };

        let body = format!("{}{};{}$y", if private { "?" } else { "" }, mode, resp);
        self.send_csi(&body);
    }

    /// `vtterm.c:CSQuote` — DEC's locator, which is a second mouse protocol
    /// sharing one state variable with the xterm family.
    fn csi_quote(&mut self, params: &Params, action: char) {
        match action {
            // DECEFR — enable filter rectangle. A zero edge means "the
            // locator's current position", so `CSI ' w` alone arms a
            // one-cell rectangle around wherever it is.
            'w' => {
                if self.mouse.tracking != Tracking::DecLocator {
                    return;
                }
                let (last_x, last_y) = self.mouse.last;
                let (x, y) = if self.mouse.locator_flags & mouse::PIXEL != 0 {
                    (last_x + 1, last_y + 1)
                } else {
                    let (cx, cy) = self.win_to_screen(last_x, last_y);
                    (cx + 1, cy + 1)
                };
                let edge = |n, dflt| match arg0(params, n) {
                    0 => dflt,
                    v => v as i32,
                };
                let (mut top, mut left) = (edge(0, y), edge(1, x));
                let (mut bottom, mut right) = (edge(2, y), edge(3, x));
                if top > bottom {
                    std::mem::swap(&mut top, &mut bottom);
                }
                if left > right {
                    std::mem::swap(&mut left, &mut right);
                }
                self.mouse.filter = (top, left, bottom, right);
                self.mouse.locator_flags |= mouse::FILTERED;
                self.locator_report(MouseEvent::Move, 0);
            }
            // DECELR — enable locator reporting. The second parameter picks
            // cell or pixel coordinates and is re-read on every call, so a
            // bare `CSI 0'z` also drops back to cells.
            'z' => {
                match arg0(params, 0) {
                    0 if self.mouse.tracking == Tracking::DecLocator => {
                        self.mouse.tracking = Tracking::None;
                    }
                    v @ (1 | 2) if self.config.mouse_tracking_enabled => {
                        self.mouse.tracking = Tracking::DecLocator;
                        if v == 1 {
                            self.mouse.locator_flags &= !mouse::ONE_SHOT;
                        } else {
                            self.mouse.locator_flags |= mouse::ONE_SHOT;
                        }
                    }
                    _ => {}
                }
                if params.len() > 1 && arg0(params, 1) == 1 {
                    self.mouse.locator_flags |= mouse::PIXEL;
                } else {
                    self.mouse.locator_flags &= !mouse::PIXEL;
                }
            }
            // DECSLE — which locator events to report.
            '{' => {
                for group in params.iter() {
                    match group.first().copied().unwrap_or(0) {
                        0 => {
                            self.mouse.locator_flags &=
                                !(mouse::BUTTON_UP | mouse::BUTTON_DOWN | mouse::FILTERED)
                        }
                        1 => self.mouse.locator_flags |= mouse::BUTTON_DOWN,
                        2 => self.mouse.locator_flags &= !mouse::BUTTON_DOWN,
                        3 => self.mouse.locator_flags |= mouse::BUTTON_UP,
                        4 => self.mouse.locator_flags &= !mouse::BUTTON_UP,
                        _ => {}
                    }
                }
            }
            // DECRQLP — where is the locator? Answers even when no locator is
            // enabled, with the `0&w` that says so.
            '|' => {
                self.locator_report(MouseEvent::CurStat, 0);
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
            'A' => self.grid.move_up(arg(params, 0, 1) as usize, true),
            'B' => self.grid.move_down(arg(params, 0, 1) as usize, true),
            'C' => self.grid.move_right(arg(params, 0, 1) as usize, true),
            'D' => self.grid.move_left(arg(params, 0, 1) as usize, true),
            // CNL and CPL move to the *left margin*, and do it before the
            // vertical move rather than after — `vtterm.c:1691`.
            'E' => {
                let (left, _) = self.grid.margins();
                self.grid.move_cursor(left, self.grid.cursor.y);
                self.grid.move_down(arg(params, 0, 1) as usize, true);
            }
            'F' => {
                let (left, _) = self.grid.margins();
                self.grid.move_cursor(left, self.grid.cursor.y);
                self.grid.move_up(arg(params, 0, 1) as usize, true);
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
                (false, mode) => {
                    self.grid.erase_display(mode);
                    // `DECSET 8200` homes the cursor after `ED 2` — to the
                    // region origin, or the screen origin under origin mode
                    // (`vtterm.c:1749`).
                    if mode == 2 && self.modes.clear_then_home {
                        if self.grid.origin_mode {
                            self.grid.move_cursor(0, 0);
                        } else {
                            let (top, _) = self.grid.scroll_region();
                            let (left, _) = self.grid.margins();
                            self.grid.move_cursor(left, top);
                        }
                    }
                }
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
                    self.primary_da();
                }
            }
            // HPR and VPR — `vtterm.c:4096` and `:4100`. The same motion as CUF
            // and CUD against the page rather than the margins.
            'a' => self.grid.move_right(arg(params, 0, 1) as usize, false),
            'e' => self.grid.move_down(arg(params, 0, 1) as usize, false),
            // HPB and VPB — `vtterm.c:4105` and `:4106`, the backwards pair.
            'j' => self.grid.move_left(arg(params, 0, 1) as usize, false),
            'k' => self.grid.move_up(arg(params, 0, 1) as usize, false),
            // VPA. Origin mode applies, which is what separates it from a bare
            // cursor move — `vtterm.c:CSMoveToLineN`.
            'd' => {
                let y = arg(params, 0, 1).saturating_sub(1) as usize;
                self.grid.move_to_row(y);
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
            // DECDSR, the private form — `vtterm.c:CSQ_n_Mode`. Two requests
            // and no more: everything else under `?` is left alone, *including*
            // 5 and 6. Answering those here would be answering DSR to a
            // question that was not DSR, and a host that asked `CSI ? 6 n`
            // (DECXCPR) wants a page number in the reply.
            'n' if private => match arg0(params, 0) {
                53 | 55 => self.send_csi("?50n"),
                _ => {}
            },
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
        self.saved_charset[usize::from(self.alt_screen)] = Some(self.charset.save());
    }

    fn restore_cursor(&mut self) {
        self.grid.restore_cursor();
        if let Some(s) = self.saved_charset[usize::from(self.alt_screen)] {
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
        if let Some(log) = &mut self.log_text {
            log.push(c);
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
                if let Some(log) = &mut self.log_text {
                    log.push('\n');
                }
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
            // DECRQCRA. Not upstream's — see `Config::decrqcra`.
            (Some(b'*'), 'y') => self.decrqcra(params),
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
            (Some(b'\''), _) => self.csi_quote(params, action),
            // DECRQM. `vte` puts the private marker in the intermediates, so
            // `CSI ? Ps $ p` arrives as `?$` and would otherwise fall through
            // to the plain path as a bare `p`.
            (Some(b'?'), 'p') if intermediates.get(1) == Some(&b'$') => {
                self.decrqm(arg0(params, 0), true)
            }
            (Some(b'$'), 'p') => self.decrqm(arg0(params, 0), false),
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
            // IND. The full `LineFeed`, LNM tail included.
            b'D' => self.line_feed(),
            // NEL. `MoveCursor(0, CursorY)` and then a line feed
            // (`vtterm.c:1508`) — **column zero**, not the left margin and not
            // `CarriageReturn`, which is the one place the two differ.
            b'E' => {
                self.grid.move_cursor(0, self.grid.cursor.y);
                self.line_feed();
            }
            // DECID, the obsolete spelling of Primary DA — `vtterm.c:1539`
            // hands it to the same `AnswerTerminalType`.
            b'Z' => self.primary_da(),
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
                        self.grid.move_left(1, true);
                    }
                }
            }
            b'9' => {
                if self.cursor_in_region() {
                    if self.grid.cursor.x == self.grid.margins().1 {
                        self.grid.scroll_left(1);
                    } else {
                        self.grid.move_right(1, true);
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
                self.saved_charset = [None, None];
                self.title.clear();
                self.title_stack.clear();
                self.rect_mode = false;
                self.lr_margin_mode = false;
                // `ResetTerminal` ends with `ChangeTerminalID()`
                // (`vtterm.c:5850`), which recomputes both from the terminal
                // *id* — so RIS undoes a DECSCL that lowered the level, and
                // undoes an S8C1T only as far as the configured default.
                self.vt_level = self.config.term_id.vt_level();
                self.send_8bit = self.vt_level >= 2 && self.config.send_8bit_ctrl;
                // `ResetTerminal` clears the mouse state outright, position
                // and button mask included. `SoftReset` clears none of it.
                self.mouse.reset();
                self.modes = self.modes.reset(&self.config);
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

    /// Drives a stream, then a sequence of mouse events, and returns the reply.
    fn mouse(stream: &[u8], events: &[(MouseEvent, u8, i32, i32)], mods: Modifiers) -> Vec<u8> {
        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 6,
            ..Config::default()
        });
        vt.feed(stream);
        let _ = vt.take_reply();
        for &(e, b, x, y) in events {
            vt.mouse_event(e, b, x, y, mods);
        }
        vt.take_reply()
    }

    #[test]
    fn mouse_tracking_is_off_until_asked_for() {
        let out = mouse(b"", &[(MouseEvent::Press, 0, 24, 80)], Modifiers::default());
        assert!(out.is_empty());
    }

    #[test]
    fn ctrl_suppresses_the_report_so_the_user_can_still_select() {
        // `ts.DisableMouseTrackingByCtrl` defaults on (ttset.c:1591).
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        let out = mouse(b"\x1b[?1000h", &[(MouseEvent::Press, 0, 24, 80)], ctrl);
        assert!(out.is_empty());

        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 6,
            disable_mouse_tracking_by_ctrl: false,
            ..Config::default()
        });
        vt.feed(b"\x1b[?1000h\x1b[?1006h");
        vt.mouse_event(MouseEvent::Press, 0, 24, 80, ctrl);
        assert_eq!(vt.take_reply(), b"\x1b[<16;4;6M");
    }

    #[test]
    fn the_report_position_follows_the_cell_size() {
        let mut vt = Vt::new(Config {
            cols: 20,
            rows: 6,
            ..Config::default()
        });
        vt.feed(b"\x1b[?1000h\x1b[?1006h");
        vt.set_cell_pixels(10, 20);
        vt.mouse_event(MouseEvent::Press, 0, 25, 41, Modifiers::default());
        // 25/10 = 2, 41/20 = 2, both one-based in the report.
        assert_eq!(vt.take_reply(), b"\x1b[<0;3;3M");
    }

    #[test]
    fn mouse_state_is_visible_to_the_frontend() {
        let mut vt = Vt::new(Config::default());
        assert_eq!(vt.mouse_tracking(), Tracking::None);
        vt.feed(b"\x1b[?1003h\x1b[?1006h\x1b[?1004h");
        assert_eq!(vt.mouse_tracking(), Tracking::AllEvent);
        assert_eq!(vt.mouse_encoding(), Encoding::Sgr);
        assert!(vt.focus_reporting());
    }

    #[test]
    fn the_setting_can_switch_mouse_tracking_off_entirely() {
        let mut vt = Vt::new(Config {
            mouse_tracking_enabled: false,
            ..Config::default()
        });
        vt.feed(b"\x1b[?1000h\x1b[?1006h\x1b[?1004h");
        assert_eq!(vt.mouse_tracking(), Tracking::None);
        assert!(!vt.focus_reporting());
        vt.mouse_event(MouseEvent::Press, 0, 24, 80, Modifiers::default());
        vt.focus_event(true);
        assert!(vt.reply().is_empty());
    }

    fn checksummer(cols: usize, rows: usize) -> Vt {
        Vt::new(Config {
            cols,
            rows,
            decrqcra: true,
            ..Config::default()
        })
    }

    #[test]
    fn decrqcra_is_silent_unless_it_is_asked_for() {
        // The default, and the faithful one: `vtterm.c` has no `CSI * y`.
        let vt = run(b"ab\x1b[42;0;1;1;1;2*y", 10, 2);
        assert!(vt.reply().is_empty());
    }

    #[test]
    fn decrqcra_sums_the_characters_in_the_rectangle() {
        let mut vt = checksummer(10, 2);
        vt.feed(b"ab\x1b[42;0;1;1;1;2*y");
        // 'a' + 'b' = 0x61 + 0x62.
        assert_eq!(vt.take_reply(), b"\x1bP42!~00C3\x1b\\");
    }

    #[test]
    fn decrqcra_reads_one_cell_as_its_own_character() {
        // The shape the whole conformance suite is built on: a single-cell
        // request must come back as exactly that character's code.
        let mut vt = checksummer(10, 2);
        vt.feed(b"Hi\x1b[7;0;1;1;1;1*y");
        assert_eq!(vt.take_reply(), b"\x1bP7!~0048\x1b\\");
    }

    #[test]
    fn decrqcra_counts_an_erased_cell_as_a_space() {
        // xterm from patch #334, and the only thing our grid can say: an
        // erase leaves a space behind, not a distinguishable "empty".
        let mut vt = checksummer(10, 2);
        vt.feed(b"X\x1b[2J\x1b[1;0;1;1;1;1*y");
        assert_eq!(vt.take_reply(), b"\x1bP1!~0020\x1b\\");
    }

    #[test]
    fn decrqcra_answers_an_inverted_rectangle_with_zero() {
        // The rectangular *operations* return silently on one. A request
        // cannot: the far end is waiting.
        let mut vt = checksummer(10, 4);
        vt.feed(b"\x1b[9;0;3;1;2;1*y");
        assert_eq!(vt.take_reply(), b"\x1bP9!~0000\x1b\\");
    }

    #[test]
    fn decrqcra_defaults_the_rectangle_to_the_whole_screen() {
        let mut vt = checksummer(4, 2);
        vt.feed(b"ab\x1b[5*y");
        // Two written cells plus six spaces.
        let want = 0x61 + 0x62 + 6 * 0x20;
        assert_eq!(
            vt.take_reply(),
            format!("\x1bP5!~{want:04X}\x1b\\").as_bytes()
        );
    }

    #[test]
    fn decrqcra_follows_the_reply_encoding_the_terminal_was_told_to_use() {
        let mut vt = Vt::new(Config {
            cols: 10,
            rows: 2,
            decrqcra: true,
            term_id: TermId::Vt320,
            ..Config::default()
        });
        // S8C1T, so the reply is wrapped in 8-bit DCS/ST rather than ESC P … ESC \.
        vt.feed(b"a\x1b G\x1b[3;0;1;1;1;1*y");
        assert_eq!(vt.take_reply(), b"\x903!~0061\x9c");
    }

    #[test]
    fn the_normal_encoding_saturates_rather_than_wrapping() {
        // MOUSE_POS_LIMIT is 223; a wider terminal simply stops counting.
        let mut vt = Vt::new(Config {
            cols: 300,
            rows: 200,
            ..Config::default()
        });
        vt.feed(b"\x1b[?1000h");
        let _ = vt.take_reply();
        vt.mouse_event(MouseEvent::Press, 0, 2392, 3184, Modifiers::default());
        // Column 300 saturates at 223 and is reported as 255; row 200 is
        // under the limit and comes through as 232.
        assert_eq!(vt.take_reply(), vec![0x1b, b'[', b'M', 32, 255, 232]);
    }

    /// Applying settings overwrites the modes the host set, because upstream
    /// keeps no second copy of them — the setting and the mode are the same
    /// variable there.
    #[test]
    fn applying_settings_overwrites_what_the_host_set() {
        let mut vt = Vt::new(Config::default());
        // SRM, DECBKM and LNM, each writing a `ts` field upstream.
        vt.feed(b"\x1b[?67l\x1b[12l\x1b[20h");
        assert!(vt.local_echo());
        assert!(!vt.backspace_sends_bs());
        assert_eq!(vt.state.modes.cr_send, CrSend::CrLf);
        // LFMode is upstream's own variable rather than a view of `ts`.
        assert!(vt.newline_mode());

        vt.set_config(Config::default());
        assert!(!vt.local_echo());
        assert!(vt.backspace_sends_bs());
        assert_eq!(vt.state.modes.cr_send, CrSend::Cr);
        // ...so it survives, and that asymmetry is `SetupTerm`'s.
        assert!(vt.newline_mode());
    }

    #[test]
    fn applying_settings_resizes_and_re_derives_the_vt_level() {
        let mut vt = Vt::new(Config::default());
        vt.feed(b"hello");
        vt.set_config(Config {
            cols: 100,
            rows: 40,
            term_id: TermId::Vt320,
            scrollback_max: 4,
            ..Config::default()
        });
        assert_eq!((vt.grid().cols(), vt.grid().rows()), (100, 40));
        assert_eq!(vt.grid().scrollback_max(), 4);
        assert_eq!(row(&vt, 0), "hello");
        vt.feed(b"\x1b[c");
        assert_eq!(vt.take_reply(), b"\x1b[?63;1;2;6;7;8;9c");
    }
}
