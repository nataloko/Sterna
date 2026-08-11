//! The colour-control OSCs: `OSC 4`/`5`/`10`-`19` and their `104`/`105`/
//! `110`-`119` resets.
//!
//! Three upstream functions are ported here — `vtterm.c`'s `XsParseColor` and
//! `XtColor2TTColor`, and `vtdisp.c`'s `DispSetColor`/`DispGetColor`/
//! `DispResetColor` — and the split between the two files is the reason this is
//! its own module. `vtdisp.c` is **not compiled into the oracle**, so the
//! differential suite cannot arbitrate any of it and neither can `esctest`;
//! everything here is read off upstream and cited, which is the same standard
//! `DispFindClosestColor` is held to next door in [`crate::palette`] after a
//! stub of it was found lying.
//!
//! The store is upstream's, both halves. `vtdraw_t` holds six live pairs and a
//! live 256-entry table, and `ts` holds the configured ones the reset returns
//! to — so [`Colors`] is the first and [`crate::Config`] is the second.

use crate::palette::Rgb;

/// `vtdisp.h:61`'s `CS_UNSPEC` — the colour number upstream passes when the OSC
/// carried none. See [`slot_of`] for why it is spelt as a value and not as an
/// `Option`.
pub const UNSPEC: u32 = 0xffff_ffff;

/// The slots `XtColor2TTColor` (`vtterm.c:4816`) can name.
///
/// `vtdisp.h:45` has fourteen `CS_*` colours and two "all" pseudo-colours; the
/// OSCs reach nine of the fourteen. There is deliberately no variant for the
/// other five — `CS_VT_BLINKBG`, `CS_VT_REVERSEFG`, `CS_VT_URLFG`,
/// `CS_VT_URLBG` and `CS_VT_UNDERBG` are settable from a colour *theme* and not
/// from the wire, so a variant for them here would be a slot nothing can select
/// and a place for a future reader to assume otherwise.
///
/// Two of the mappings look like typos and are not. `CSI ] 17` — xterm's
/// *highlight background* — is `CS_VT_BOLDBG`, and `OSC 19`, its highlight
/// foreground, is `CS_VT_BOLDFG`: Tera Term draws its selection with the bold
/// pair, so those are the colours a host asking about the highlight is asking
/// about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    NormalFg,
    NormalBg,
    BoldFg,
    BoldBg,
    BlinkFg,
    ReverseBg,
    UnderFg,
    TekFg,
    TekBg,
    /// One entry of the 256-colour table.
    Ansi(u8),
    /// `CS_ANSICOLOR_ALL`. Reset only — `OSC 104` with no parameter string.
    AnsiAll,
    /// `CS_SP_ALL`. Reset only — `OSC 105` with no parameter string, and it
    /// does **not** mean every special colour. See [`Colors::reset`].
    SpecialAll,
}

/// `vtterm.c:XtColor2TTColor` — which slot an OSC number and its colour number
/// select. `None` is upstream's `CS_UNSPEC`, which every caller treats as "do
/// nothing".
///
/// `mode` is the OSC's own number, and a reset number is folded onto the setter
/// it undoes by upstream's `(mode>=100) ? mode-100 : mode`.
///
/// [`UNSPEC`] as the colour number is upstream's "no number was given", and it
/// is a **sentinel rather than a flag** — which matters, because it is also a
/// number a host can send. Only `104` and `105` have an arm for it, so
/// `OSC 105;4294967295` resets every special colour exactly as a bare `OSC 105`
/// does, while a `4` or `5` carrying it selects nothing.
///
/// Four xterm colours have no arm at all — 12 (text cursor), 13 and 14 (mouse
/// pointer) and 18 (Tek cursor) — so `OSC 12;#ff0000` is silently ignored, as
/// is `OSC 112`. That is worth knowing before reading a cursor colour that
/// never arrives as a bug in the frontend.
pub fn slot_of(mode: u32, color: u32) -> Option<Slot> {
    let base = if mode >= 100 { mode - 100 } else { mode };
    match base {
        4 => match color {
            256 => Some(Slot::BoldFg),
            257 => Some(Slot::UnderFg),
            258 => Some(Slot::BlinkFg),
            // `// xterm と同じ動作` — xterm's 259 is the reverse-video
            // background, so this is the one of the four that is not simply
            // the next slot along.
            259 => Some(Slot::ReverseBg),
            UNSPEC => (mode == 104).then_some(Slot::AnsiAll),
            _ if color <= 255 => Some(Slot::Ansi(color as u8)),
            _ => None,
        },
        5 => match color {
            0 => Some(Slot::BoldFg),
            1 => Some(Slot::UnderFg),
            2 => Some(Slot::BlinkFg),
            3 => Some(Slot::ReverseBg),
            UNSPEC => (mode == 105).then_some(Slot::SpecialAll),
            _ => None,
        },
        10 => Some(Slot::NormalFg),
        11 => Some(Slot::NormalBg),
        15 => Some(Slot::TekFg),
        16 => Some(Slot::TekBg),
        17 => Some(Slot::BoldBg),
        19 => Some(Slot::BoldFg),
        _ => None,
    }
}

/// `vtterm.c:XsParseColor` — an X11 colour specification, in the two forms
/// upstream accepts.
///
/// `rgb:R/G/B` in one to four hex digits per channel, and `#RGB` in the same
/// four widths. **Everything else is refused**, including every form `esctest`
/// asks about beyond those two: `rgbi:` is present in the source as a commented
/// -out arm (`:4773`) and `CIELab:`, `CIEXYZ:`, `TekHVC:` and the rest were
/// never written, so a host using one of them gets no colour change and no
/// reply.
///
/// The widths do not scale the way X11 says they do, and that is visible in a
/// query. `#f00` is `<< 4` — 0xF0, not 0xFF — so setting a colour in the short
/// form and reading it back gives `rgb:f0f0/0000/0000` where xterm answers
/// `rgb:ffff/0000/0000`. The wide forms truncate rather than round.
///
/// **`rgb:` is matched case-insensitively and then parsed case-sensitively.**
/// The guard is `_strnicmp` and the parse is `sscanf` against a literal `rgb:`,
/// which matches exactly — so `RGB:0/0/0` passes the first test, fails the
/// second and is rejected, while `#` has no such split and takes any case of
/// hex digit. Nothing announces it; the colour simply does not change.
pub fn parse_spec(spec: &[u8]) -> Option<Rgb> {
    let (r, g, b) = if spec.len() >= 4 && spec[..4].eq_ignore_ascii_case(b"rgb:") {
        // The `sscanf` literal is lower-case and matches as one, so a spec that
        // only got here by the case-insensitive guard stops at the first byte.
        if !spec.starts_with(b"rgb:") {
            return None;
        }
        let (width, shift) = match spec.len() {
            9 => (1, 0),
            12 => (2, 0),
            15 => (3, 4),
            18 => (4, 8),
            _ => return None,
        };
        let mut fields = [0u32; 3];
        let mut at = 4;
        for (i, field) in fields.iter_mut().enumerate() {
            if i > 0 {
                if spec.get(at) != Some(&b'/') {
                    return None;
                }
                at += 1;
            }
            let (value, used) = scanf_hex(&spec[at..], width)?;
            *field = value;
            at += used;
        }
        let [r, g, b] = fields;
        if width == 1 {
            // `r *= 17` — the one width that scales, so `rgb:f/0/0` is a true
            // 0xFF where `#f00` is 0xF0.
            (r * 17, g * 17, b * 17)
        } else {
            (r >> shift, g >> shift, b >> shift)
        }
    } else if spec.first() == Some(&b'#') {
        let (width, shift) = match spec.len() {
            4 => (1, -4),
            7 => (2, 0),
            10 => (3, 4),
            13 => (4, 8),
            _ => return None,
        };
        let mut fields = [0u32; 3];
        let mut at = 1;
        for field in fields.iter_mut() {
            let (value, used) = scanf_hex(&spec[at..], width)?;
            *field = value;
            at += used;
        }
        let [r, g, b] = fields;
        if shift < 0 {
            (r << 4, g << 4, b << 4)
        } else {
            (r >> shift, g >> shift, b >> shift)
        }
    } else {
        return None;
    };

    // `if (r > 255 || g > 255 || b > 255) return FALSE;`. It reads like the
    // dead clamp `AlphaBlend`'s narrowing hides, and it is not: `%x` takes an
    // optional sign, so `rgb:-1/-1/-1` fits the twelve-byte form, converts to
    // 0xffffffff and is refused here rather than at the parse.
    if r > 255 || g > 255 || b > 255 {
        return None;
    }
    Some((r as u8, g as u8, b as u8))
}

/// `sscanf`'s `%<width>x`: leading whitespace, an optional sign, then at most
/// `width` hex digits. Returns the value and how many bytes it took, or `None`
/// where `sscanf` would have stopped short of its three conversions.
///
/// The sign is not decoration — see the range test in [`parse_spec`], which is
/// only reachable through it. A negative converts the way `strtoul` does, by
/// wrapping, so `-1` is 0xffffffff and not an error.
fn scanf_hex(buf: &[u8], width: usize) -> Option<(u32, usize)> {
    let mut i = 0;
    while matches!(buf.get(i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        i += 1;
    }
    let negative = match buf.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut value: u32 = 0;
    while i - start < width {
        let Some(digit) = buf.get(i).and_then(|b| (*b as char).to_digit(16)) else {
            break;
        };
        value = value.wrapping_mul(16).wrapping_add(digit);
        i += 1;
    }
    if i == start {
        return None;
    }
    Some((
        if negative {
            value.wrapping_neg()
        } else {
            value
        },
        i,
    ))
}

/// The live colours — `vtdraw_t`'s `BG*` pairs and its `ANSIColor[256]`.
///
/// Held in the terminal rather than in the painter for the reason
/// [`crate::Config::palette`] gives: the 256 entries decide which *index*
/// truecolor SGR stores, so a host that repaints the palette changes the grid
/// and not only how it looks.
///
/// Each pair is `[foreground, background]`, upstream's own array order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Colors {
    pub ansi: [Rgb; 256],
    pub normal: [Rgb; 2],
    pub bold: [Rgb; 2],
    pub blink: [Rgb; 2],
    pub reverse: [Rgb; 2],
    pub url: [Rgb; 2],
    pub underline: [Rgb; 2],
    /// Upstream has no live copy of this pair: `DispSetColor` writes
    /// `ts.TEKColor` itself (`vtdisp.c:3422`) and `DispResetColor`'s two Tek
    /// arms are empty, so a host that sets a Tek colour has overwritten the
    /// value a reset would have returned to and **Save setup would write it to
    /// the user's `TERATERM.INI`**. The wire cannot tell the two designs apart
    /// — a query reads back whatever was set either way, and the reset does
    /// nothing either way — so the copy is here and the settings file is left
    /// alone.
    pub tek: [Rgb; 2],
}

impl Colors {
    /// `vtdisp.c:InitColorTable` plus the six pairs `BGInitialize` copies out of
    /// `ts`.
    ///
    /// The table arrives already permuted: [`crate::Config::palette`] is what
    /// `InitColorTable` produces, not the sixteen-entry `ts.ANSIColor` it reads,
    /// so there is no bright/dim swap left to do here.
    pub fn new(config: &crate::Config) -> Colors {
        Colors {
            ansi: config.palette,
            normal: config.color_normal,
            bold: config.color_bold,
            blink: config.color_blink,
            reverse: config.color_reverse,
            url: config.color_url,
            underline: config.color_underline,
            tek: config.color_tek,
        }
    }

    /// `vtdisp.c:DispSetColor`.
    ///
    /// The eight-colour arm is upstream's: with `Xterm256Color` off, a palette
    /// index is taken as a *legacy* index and permuted before it is stored, so
    /// `OSC 4;1;#00ff00` paints the bright red slot instead of the dim one.
    /// `AnsiAll` and `SpecialAll` cannot be reached from a setter, since
    /// [`slot_of`] only produces them for `104` and `105`.
    pub fn set(&mut self, slot: Slot, color: Rgb, full_color: bool) {
        match slot {
            Slot::NormalFg => self.normal[0] = color,
            Slot::NormalBg => self.normal[1] = color,
            Slot::BoldFg => self.bold[0] = color,
            Slot::BoldBg => self.bold[1] = color,
            Slot::BlinkFg => self.blink[0] = color,
            Slot::ReverseBg => self.reverse[1] = color,
            Slot::UnderFg => self.underline[0] = color,
            Slot::TekFg => self.tek[0] = color,
            Slot::TekBg => self.tek[1] = color,
            Slot::Ansi(index) => {
                self.ansi[index_for(index, full_color)] = color;
            }
            Slot::AnsiAll | Slot::SpecialAll => {}
        }
    }

    /// `vtdisp.c:DispGetColor`, and **the surprise is that a query does not see
    /// what a set did.**
    ///
    /// Every special colour is read back out of `ts` — the *settings* — while
    /// `DispSetColor` wrote the live `vtdraw_t` copy, so
    /// `OSC 10;#ff0000` followed by `OSC 10;?` answers with the configured
    /// foreground and not with red. Only the palette round-trips, because both
    /// halves of it are `vt->ANSIColor`, and Tek does, because upstream's setter
    /// happens to write the same `ts` field the getter reads.
    ///
    /// It is upstream's shape rather than a transcription slip, so it is
    /// reproduced; it is also the reason `esctest`'s `ChangeDynamicColor` cases
    /// cannot pass here however the parser is written.
    pub fn get(&self, slot: Slot, config: &crate::Config, full_color: bool) -> Rgb {
        match slot {
            Slot::NormalFg => config.color_normal[0],
            Slot::NormalBg => config.color_normal[1],
            Slot::BoldFg => config.color_bold[0],
            Slot::BoldBg => config.color_bold[1],
            Slot::BlinkFg => config.color_blink[0],
            Slot::ReverseBg => config.color_reverse[1],
            Slot::UnderFg => config.color_underline[0],
            Slot::TekFg => self.tek[0],
            Slot::TekBg => self.tek[1],
            Slot::Ansi(index) => self.ansi[index_for(index, full_color)],
            // Unreachable through `XsProcColor`, which tests for `CS_UNSPEC`
            // before it queries; upstream's `default` arm would answer with
            // `vt->ANSIColor[0]`.
            Slot::AnsiAll | Slot::SpecialAll => self.ansi[0],
        }
    }

    /// `vtdisp.c:DispResetColor` — put a slot back to what the settings say.
    ///
    /// Three of its arms are not what their names promise:
    ///
    /// - **`SpecialAll` restores three colours, not the special set.** Bold
    ///   foreground, blink foreground and reverse background — exactly the
    ///   three `OSC 5` can address minus the underline foreground, which it can
    ///   also address and which `OSC 105` therefore cannot undo.
    /// - **Tek resets nothing.** Both Tek arms are empty `break`s.
    /// - **An entry above 15 goes back to the built-in cube**, not to anything
    ///   configurable, which is the same value because `ANSIColor` masks its
    ///   colour id to four bits and cannot reach past 15 in the first place.
    pub fn reset(&mut self, slot: Slot, config: &crate::Config, full_color: bool) {
        match slot {
            Slot::NormalFg => self.normal[0] = config.color_normal[0],
            Slot::NormalBg => self.normal[1] = config.color_normal[1],
            Slot::BoldFg => self.bold[0] = config.color_bold[0],
            Slot::BoldBg => self.bold[1] = config.color_bold[1],
            Slot::BlinkFg => self.blink[0] = config.color_blink[0],
            Slot::ReverseBg => self.reverse[1] = config.color_reverse[1],
            Slot::UnderFg => self.underline[0] = config.color_underline[0],
            Slot::TekFg | Slot::TekBg => {}
            Slot::Ansi(index) => {
                let i = index_for(index, full_color);
                self.ansi[i] = config.palette[i];
            }
            Slot::AnsiAll => self.ansi = config.palette,
            Slot::SpecialAll => {
                self.bold[0] = config.color_bold[0];
                self.blink[0] = config.color_blink[0];
                self.reverse[1] = config.color_reverse[1];
            }
        }
    }
}

/// `GetIndex256From16` (`vtdisp.c:1400`), applied where upstream applies it.
///
/// With any full-colour mode on, a palette index is the drawing index and
/// nothing moves. With all of them off the terminal is in its eight-colour
/// mode, where the wire's index is the *legacy* one — bright and dim swapped —
/// and has to be permuted into the drawing table. The permutation is its own
/// inverse, which is why upstream's `GetIndex16From256` is one line calling it.
fn index_for(index: u8, full_color: bool) -> usize {
    if full_color || index > 15 {
        return usize::from(index);
    }
    const INDEX_256: [usize; 16] = [0, 9, 10, 11, 12, 13, 14, 15, 8, 1, 2, 3, 4, 5, 6, 7];
    INDEX_256[usize::from(index)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_rgb_widths_scale_the_way_upstream_scales_them() {
        assert_eq!(parse_spec(b"rgb:f/0/0"), Some((255, 0, 0)));
        assert_eq!(parse_spec(b"rgb:ff/00/00"), Some((255, 0, 0)));
        assert_eq!(parse_spec(b"rgb:fff/000/000"), Some((255, 0, 0)));
        assert_eq!(parse_spec(b"rgb:ffff/0000/0000"), Some((255, 0, 0)));
    }

    #[test]
    fn the_short_hash_form_shifts_rather_than_repeating_the_digit() {
        // xterm answers 0xFF here; upstream's `<< 4` answers 0xF0, and the
        // difference is visible in a query's reply.
        assert_eq!(parse_spec(b"#f00"), Some((0xf0, 0, 0)));
        assert_eq!(parse_spec(b"#ff0000"), Some((255, 0, 0)));
        assert_eq!(parse_spec(b"#fff000000"), Some((255, 0, 0)));
        assert_eq!(parse_spec(b"#ffff00000000"), Some((255, 0, 0)));
    }

    #[test]
    fn a_length_that_is_not_one_of_the_four_is_refused() {
        assert_eq!(parse_spec(b"rgb:ff/00/0"), None);
        assert_eq!(parse_spec(b"#ff000"), None);
        assert_eq!(parse_spec(b""), None);
    }

    #[test]
    fn the_forms_upstream_never_implemented_are_refused() {
        // The arm for this one is in the source, commented out.
        assert_eq!(parse_spec(b"rgbi:1.0/0.0/0.0"), None);
        assert_eq!(parse_spec(b"CIELab:50.0/0.0/0.0"), None);
        assert_eq!(parse_spec(b"TekHVC:0.0/50.0/0.0"), None);
        assert_eq!(parse_spec(b"red"), None);
    }

    #[test]
    fn an_upper_case_rgb_passes_the_guard_and_fails_the_parse() {
        assert_eq!(parse_spec(b"rgb:AB/CD/EF"), Some((0xab, 0xcd, 0xef)));
        assert_eq!(parse_spec(b"RGB:ab/cd/ef"), None);
        assert_eq!(parse_spec(b"#ABCDEF"), Some((0xab, 0xcd, 0xef)));
    }

    #[test]
    fn a_signed_field_is_what_makes_the_range_test_reachable() {
        // `%2x` takes the sign, wraps to 0xffffffff, and the `> 255` test that
        // looks dead is what refuses it.
        assert_eq!(parse_spec(b"rgb:-1/-1/-1"), None);
        assert_eq!(parse_spec(b"rgb:+1/+1/+1"), Some((1, 1, 1)));
    }

    #[test]
    fn a_missing_separator_stops_the_conversion() {
        assert_eq!(parse_spec(b"rgb:ff:00:00"), None);
    }

    #[test]
    fn the_reset_numbers_fold_onto_the_setters_they_undo() {
        assert_eq!(slot_of(10, 0), Some(Slot::NormalFg));
        assert_eq!(slot_of(110, UNSPEC), Some(Slot::NormalFg));
        assert_eq!(slot_of(17, 0), Some(Slot::BoldBg));
        assert_eq!(slot_of(19, 0), Some(Slot::BoldFg));
    }

    #[test]
    fn the_four_xterm_colours_with_no_arm_select_nothing() {
        for mode in [12, 13, 14, 18] {
            assert_eq!(slot_of(mode, 0), None, "OSC {mode}");
            assert_eq!(slot_of(mode + 100, UNSPEC), None, "OSC {}", mode + 100);
        }
    }

    #[test]
    fn only_the_reset_numbers_reach_the_two_all_slots() {
        assert_eq!(slot_of(104, UNSPEC), Some(Slot::AnsiAll));
        assert_eq!(slot_of(105, UNSPEC), Some(Slot::SpecialAll));
        assert_eq!(slot_of(4, UNSPEC), None);
        assert_eq!(slot_of(5, UNSPEC), None);
        // With a number rather than without one, 104 is an ordinary entry.
        assert_eq!(slot_of(104, 0), Some(Slot::Ansi(0)));
    }

    #[test]
    fn the_no_number_sentinel_is_a_number_a_host_can_send() {
        // `OSC 105;4294967295` is a bare `OSC 105`, and `OSC 5;4294967295;x`
        // selects nothing at all.
        assert_eq!(slot_of(105, UNSPEC), Some(Slot::SpecialAll));
        assert_eq!(slot_of(5, UNSPEC), None);
        assert_eq!(slot_of(4, UNSPEC), None);
    }

    #[test]
    fn osc_four_reaches_the_four_special_colours_above_the_palette() {
        assert_eq!(slot_of(4, 255), Some(Slot::Ansi(255)));
        assert_eq!(slot_of(4, 256), Some(Slot::BoldFg));
        assert_eq!(slot_of(4, 257), Some(Slot::UnderFg));
        assert_eq!(slot_of(4, 258), Some(Slot::BlinkFg));
        assert_eq!(slot_of(4, 259), Some(Slot::ReverseBg));
        assert_eq!(slot_of(4, 260), None);
    }

    #[test]
    fn eight_colour_mode_permutes_a_palette_index_and_full_colour_does_not() {
        assert_eq!(index_for(1, true), 1);
        assert_eq!(index_for(1, false), 9);
        assert_eq!(index_for(9, false), 1);
        assert_eq!(index_for(0, false), 0);
        assert_eq!(index_for(8, false), 8);
        assert_eq!(index_for(200, false), 200);
    }

    fn config() -> crate::Config {
        crate::Config::default()
    }

    #[test]
    fn a_query_of_a_special_colour_does_not_see_what_a_set_did() {
        let c = config();
        let mut colors = Colors::new(&c);
        colors.set(Slot::NormalFg, (1, 2, 3), true);
        assert_eq!(colors.normal[0], (1, 2, 3));
        assert_eq!(colors.get(Slot::NormalFg, &c, true), c.color_normal[0]);
        // The palette is the half that does round-trip.
        colors.set(Slot::Ansi(42), (1, 2, 3), true);
        assert_eq!(colors.get(Slot::Ansi(42), &c, true), (1, 2, 3));
    }

    #[test]
    fn special_all_puts_back_three_colours_and_leaves_the_underline() {
        let c = config();
        let mut colors = Colors::new(&c);
        for slot in [Slot::BoldFg, Slot::BlinkFg, Slot::ReverseBg, Slot::UnderFg] {
            colors.set(slot, (1, 2, 3), true);
        }
        colors.reset(Slot::SpecialAll, &c, true);
        assert_eq!(colors.bold[0], c.color_bold[0]);
        assert_eq!(colors.blink[0], c.color_blink[0]);
        assert_eq!(colors.reverse[1], c.color_reverse[1]);
        // `OSC 5;1` can set it and `OSC 105` cannot put it back.
        assert_eq!(colors.underline[0], (1, 2, 3));
    }

    #[test]
    fn a_tek_colour_cannot_be_reset() {
        let c = config();
        let mut colors = Colors::new(&c);
        colors.set(Slot::TekFg, (1, 2, 3), true);
        colors.reset(Slot::TekFg, &c, true);
        assert_eq!(colors.tek[0], (1, 2, 3));
        assert_eq!(colors.get(Slot::TekFg, &c, true), (1, 2, 3));
    }

    #[test]
    fn resetting_the_whole_palette_puts_back_the_configured_table() {
        let mut c = config();
        c.palette[1] = (9, 9, 9);
        let mut colors = Colors::new(&c);
        colors.set(Slot::Ansi(1), (1, 2, 3), true);
        colors.set(Slot::Ansi(200), (1, 2, 3), true);
        colors.reset(Slot::AnsiAll, &c, true);
        assert_eq!(colors.ansi[1], (9, 9, 9));
        assert_eq!(colors.ansi[200], c.palette[200]);
    }
}
