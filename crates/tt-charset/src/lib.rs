//! tt-charset — ISO-2022 designation and invocation.
//!
//! Ported from Tera Term's `charset.cpp`, and *verified against the running
//! oracle* rather than against the ISO-2022 standard, because the two differ in
//! at least one visible way (see [`Iso2022::single_shift_pending`]).
//!
//! Only the parts that matter with CJK deferred are here: the four G-sets, the
//! locking and single shifts, and whether a byte lands in the **DEC special
//! graphics** set — which is what drives box drawing. The Katakana and Kanji
//! designations are carried as values so the state model stays complete, but
//! nothing interprets them yet.
//!
//! What this crate deliberately does *not* do is translate DEC special
//! characters to Unicode. Tera Term ships with `DecSpMappingDir` defaulting to
//! "do not map", so `ESC ( 0` followed by `q` stores the byte `q` and marks the
//! cell `AttrSpecial`; turning it into U+2500 is the renderer's job. Mapping it
//! here would be a different terminal.

/// The four character sets Tera Term can designate into a G-set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CharSet {
    #[default]
    Ascii,
    Katakana,
    Kanji,
    /// DEC Special Graphics and Line Drawing — `ESC ( 0`.
    Special,
}

/// Locking and single shifts. `charset.h:CharSet2022Shift`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shift {
    /// SI / `ESC ~`-family sibling: G0 → GL.
    Ls0,
    /// SO: G1 → GL.
    Ls1,
    /// `ESC n`: G2 → GL.
    Ls2,
    /// `ESC o`: G3 → GL.
    Ls3,
    /// `ESC ~`: G1 → GR.
    Ls1r,
    /// `ESC }`: G2 → GR.
    Ls2r,
    /// `ESC |`: G3 → GR.
    Ls3r,
    /// `ESC N`: G2 for one character.
    Ss2,
    /// `ESC O`: G3 for one character.
    Ss3,
}

/// Which shifts the terminal honours. Tera Term's `ts.ISO2022Flag`.
///
/// **The default is every shift enabled.** `ttset.c:1875` reads the
/// `ISO2022ShiftFunction` key with a default string of `"on"`, which resolves to
/// `ISO2022_SHIFT_ALL` — the `ISO2022_SHIFT_NONE` a few lines above it is the
/// initialiser, not the default, and reading it as one disables SO/SI entirely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShiftFlags(pub u16);

impl ShiftFlags {
    pub const SI: u16 = 0x0001;
    pub const SO: u16 = 0x0002;
    pub const LS2: u16 = 0x0004;
    pub const LS3: u16 = 0x0008;
    pub const LS1R: u16 = 0x0010;
    pub const LS2R: u16 = 0x0020;
    pub const LS3R: u16 = 0x0040;
    pub const SS2: u16 = 0x0100;
    pub const SS3: u16 = 0x0200;

    pub const NONE: ShiftFlags = ShiftFlags(0);
    pub const ALL: ShiftFlags = ShiftFlags(
        Self::SI
            | Self::SO
            | Self::LS2
            | Self::LS3
            | Self::LS1R
            | Self::LS2R
            | Self::LS3R
            | Self::SS2
            | Self::SS3,
    );

    /// The nine names the INI spells, each with its bit — `ttset.c:1902`, and
    /// the order the writer emits them in (`:3227`).
    ///
    /// `LS0` and `LS1` are read-only aliases for `SI` and `SO`: upstream's
    /// reader takes either and its writer emits only the first.
    const NAMES: [(&'static str, u16); 11] = [
        ("SI", Self::SI),
        ("LS0", Self::SI),
        ("SO", Self::SO),
        ("LS1", Self::SO),
        ("LS2", Self::LS2),
        ("LS3", Self::LS3),
        ("LS1R", Self::LS1R),
        ("LS2R", Self::LS2R),
        ("LS3R", Self::LS3R),
        ("SS2", Self::SS2),
        ("SS3", Self::SS3),
    ];

    /// `ISO2022ShiftFunction`'s value — `ttset.c:1877`'s loop.
    ///
    /// A comma-separated list, each item optionally led by `+` or `-`, where
    /// `on`/`all` and `off`/`none` **assign** the whole word rather than
    /// setting one bit. Anything unrecognised is ignored, since upstream's
    /// chain leaves `mask` at 0 and the `if (mask)` below it declines.
    ///
    /// **It starts from nothing, not from the default**, which is the trap
    /// here: the `"on"` at the top of that call is the string used when the key
    /// is *absent*, and a key that is present starts the loop at
    /// `ISO2022_SHIFT_NONE`. So `ISO2022ShiftFunction=-SS2` is not "all but
    /// SS2" — it is a terminal with **every shift disabled**, which is a
    /// perfectly reasonable thing to write and the opposite of what it does.
    pub fn parse_ini(value: &str) -> ShiftFlags {
        let mut out = ShiftFlags::NONE;
        for item in value.split(',') {
            let item = item.trim();
            let (add, name) = match item.strip_prefix('-') {
                Some(rest) => (false, rest.trim()),
                None => (true, item.strip_prefix('+').unwrap_or(item).trim()),
            };
            if name.eq_ignore_ascii_case("on") || name.eq_ignore_ascii_case("all") {
                out = ShiftFlags::ALL;
            } else if name.eq_ignore_ascii_case("off") || name.eq_ignore_ascii_case("none") {
                out = ShiftFlags::NONE;
            } else if let Some(&(_, bit)) = Self::NAMES
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
            {
                match add {
                    true => out.0 |= bit,
                    false => out.0 &= !bit,
                }
            }
        }
        out
    }

    /// The spelling upstream's writer produces — `ttset.c:3222`. Every bit set
    /// is `on` rather than the list, and none is `off` rather than the empty
    /// string.
    pub fn to_ini(self) -> String {
        if self == ShiftFlags::ALL {
            return "on".into();
        }
        let names: Vec<&str> = Self::NAMES
            .iter()
            .filter(|(n, bit)| self.0 & bit != 0 && !matches!(*n, "LS0" | "LS1"))
            .map(|(n, _)| *n)
            .collect();
        match names.is_empty() {
            true => "off".into(),
            false => names.join(","),
        }
    }

    pub fn allows(self, shift: Shift) -> bool {
        let bit = match shift {
            Shift::Ls0 => Self::SI,
            Shift::Ls1 => Self::SO,
            Shift::Ls2 => Self::LS2,
            Shift::Ls3 => Self::LS3,
            Shift::Ls1r => Self::LS1R,
            Shift::Ls2r => Self::LS2R,
            Shift::Ls3r => Self::LS3R,
            Shift::Ss2 => Self::SS2,
            Shift::Ss3 => Self::SS3,
        };
        self.0 & bit != 0
    }
}

impl Default for ShiftFlags {
    fn default() -> Self {
        ShiftFlags::ALL
    }
}

/// The part of the state DECSC saves — `charset.cpp:CharSetSaveStateLow`, which
/// stores GL/GR and the four G-sets and pointedly does *not* store the pending
/// single shift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Iso2022State {
    glr: [usize; 2],
    gn: [CharSet; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Iso2022 {
    gn: [CharSet; 4],
    /// `[GL, GR]`, each naming a G-set by index.
    glr: [usize; 2],
    gl_tmp: usize,
    ss: bool,
}

impl Default for Iso2022 {
    fn default() -> Self {
        Iso2022::new()
    }
}

impl Iso2022 {
    /// `charset.cpp:CharSetInit2`, non-Japanese branch.
    ///
    /// **G1 starts as DEC special graphics**, which is why a bare SO switches to
    /// line drawing with no `ESC ( 0` anywhere in the stream. Software that
    /// draws boxes with SO/SI and never designates relies on exactly this.
    pub fn new() -> Self {
        Iso2022 {
            gn: [
                CharSet::Ascii,
                CharSet::Special,
                CharSet::Ascii,
                CharSet::Ascii,
            ],
            glr: [0, 0],
            gl_tmp: 0,
            ss: false,
        }
    }

    pub fn reset(&mut self) {
        *self = Iso2022::new();
    }

    /// `ESC ( ) * +` with a final byte. `gn` is 0..=3.
    pub fn designate(&mut self, gn: usize, cs: CharSet) {
        if gn < 4 {
            self.gn[gn] = cs;
        }
    }

    pub fn invoke(&mut self, shift: Shift) {
        match shift {
            Shift::Ls0 => self.glr[0] = 0,
            Shift::Ls1 => self.glr[0] = 1,
            Shift::Ls2 => self.glr[0] = 2,
            Shift::Ls3 => self.glr[0] = 3,
            Shift::Ls1r => self.glr[1] = 1,
            Shift::Ls2r => self.glr[1] = 2,
            Shift::Ls3r => self.glr[1] = 3,
            Shift::Ss2 => {
                self.gl_tmp = 2;
                self.ss = true;
            }
            Shift::Ss3 => {
                self.gl_tmp = 3;
                self.ss = true;
            }
        }
    }

    /// Is a pending single shift in effect?
    ///
    /// It is on the caller to end it — and in UTF-8 mode Tera Term never does.
    /// `ParseFirst` clears `SSflag` after one character, but `ParseFirstUTF8`
    /// returns before reaching that code, so a single `ESC N` redirects
    /// **every** subsequent character to G2 for the rest of the session.
    /// Confirmed by running the oracle, not inferred. Reproduced here because
    /// the oracle is ground truth; call [`Iso2022::end_single_shift`] from an
    /// encoding that does clear it.
    pub fn single_shift_pending(&self) -> bool {
        self.ss
    }

    pub fn end_single_shift(&mut self) {
        self.ss = false;
    }

    /// Which G-set a byte resolves through right now.
    fn active(&self, gr: bool) -> CharSet {
        if self.ss {
            self.gn[self.gl_tmp]
        } else {
            self.gn[self.glr[usize::from(gr)]]
        }
    }

    /// Does this codepoint land in DEC special graphics?
    ///
    /// `charset.cpp:CharSetIsSpecial`. Only two ranges qualify — 0x5F..=0x7E in
    /// GL and 0xDF..=0xFE in GR — so `ESC ( 0` followed by a digit or a capital
    /// letter is *not* line drawing, which is correct and surprises people.
    pub fn is_special(&self, cp: u32) -> bool {
        if (0x5f..0x7f).contains(&cp) {
            self.active(false) == CharSet::Special
        } else if (0xdf..0xff).contains(&cp) {
            self.active(true) == CharSet::Special
        } else {
            false
        }
    }

    pub fn save(&self) -> Iso2022State {
        Iso2022State {
            glr: self.glr,
            gn: self.gn,
        }
    }

    pub fn restore(&mut self, state: Iso2022State) {
        self.glr = state.glr;
        self.gn = state.gn;
    }
}

/// Map a `ESC ( ) * +` final byte to a character set. `vtterm.c:ESCSBCSSelect`.
///
/// `japanese` gates the Katakana designation exactly as
/// `LangIsJapanese(ts.KanjiCode)` does upstream; it is false for a UTF-8
/// terminal, so `ESC ( I` is a no-op there rather than an error.
pub fn sbcs_final(b: u8, japanese: bool) -> Option<CharSet> {
    match b {
        b'0' => Some(CharSet::Special),
        b'<' | b'>' | b'A' | b'B' | b'H' | b'J' => Some(CharSet::Ascii),
        b'I' if japanese => Some(CharSet::Katakana),
        _ => None,
    }
}

/// `Dist = (IntChar[1] - '(') & 3` — which G-set an intermediate byte selects.
pub fn gset_from_intermediate(b: u8) -> usize {
    (b.wrapping_sub(b'(') & 3) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list, the two whole-word assignments, and the trap in the middle.
    #[test]
    fn the_shift_list_starts_from_nothing_whatever_the_default_is() {
        let of = ShiftFlags::parse_ini;
        assert_eq!(of("on"), ShiftFlags::ALL);
        assert_eq!(of("all"), ShiftFlags::ALL);
        assert_eq!(of("off"), ShiftFlags::NONE);
        assert_eq!(of(""), ShiftFlags::NONE);

        assert_eq!(of("SI,SO").0, ShiftFlags::SI | ShiftFlags::SO);
        assert_eq!(of("LS0, LS1").0, ShiftFlags::SI | ShiftFlags::SO, "aliases");
        assert_eq!(of("si,+so,-si").0, ShiftFlags::SO, "case, and the prefixes");
        assert_eq!(of("SI,nonsense,SO").0, ShiftFlags::SI | ShiftFlags::SO);

        // The whole-word arms assign rather than merge, wherever they appear.
        assert_eq!(
            of("on,-SS2"),
            ShiftFlags(ShiftFlags::ALL.0 & !ShiftFlags::SS2)
        );
        assert_eq!(of("SI,SO,off"), ShiftFlags::NONE);

        // ...and this is the one somebody would write meaning the line above
        // it. The loop starts at NONE, so removing a bit removes it from
        // nothing: every shift is off, not all but SS2.
        assert_eq!(of("-SS2"), ShiftFlags::NONE);
    }

    #[test]
    fn the_writers_spelling_round_trips() {
        for value in [
            "on",
            "off",
            "SI,SO",
            "LS2,SS3",
            "SI,SO,LS2,LS3,LS1R,LS2R,LS3R,SS2",
        ] {
            let f = ShiftFlags::parse_ini(value);
            assert_eq!(f.to_ini(), value, "{value}");
            assert_eq!(ShiftFlags::parse_ini(&f.to_ini()), f);
        }
        // The aliases are read and not written, which is upstream's writer.
        assert_eq!(ShiftFlags::parse_ini("LS0,LS1").to_ini(), "SI,SO");
    }

    #[test]
    fn g1_defaults_to_special_so_a_bare_so_draws_lines() {
        let mut cs = Iso2022::new();
        assert!(!cs.is_special(b'q' as u32));
        cs.invoke(Shift::Ls1);
        assert!(cs.is_special(b'q' as u32));
        cs.invoke(Shift::Ls0);
        assert!(!cs.is_special(b'q' as u32));
    }

    #[test]
    fn designating_g0_special_covers_only_the_five_f_range() {
        let mut cs = Iso2022::new();
        cs.designate(0, CharSet::Special);
        assert!(cs.is_special(b'_' as u32)); // 0x5f, the first
        assert!(cs.is_special(b'q' as u32));
        assert!(cs.is_special(b'~' as u32)); // 0x7e, the last
        assert!(!cs.is_special(0x7f));
        assert!(!cs.is_special(b'A' as u32));
        assert!(!cs.is_special(b'0' as u32));
    }

    #[test]
    fn gr_range_resolves_through_gr() {
        let mut cs = Iso2022::new();
        cs.designate(3, CharSet::Special);
        assert!(!cs.is_special(0xdf));
        cs.invoke(Shift::Ls3r);
        assert!(cs.is_special(0xdf));
        assert!(cs.is_special(0xfe));
        assert!(!cs.is_special(0xff));
    }

    #[test]
    fn single_shift_sticks_because_utf8_never_ends_it() {
        let mut cs = Iso2022::new();
        cs.designate(0, CharSet::Special);
        assert!(cs.is_special(b'q' as u32));
        cs.invoke(Shift::Ss2); // G2 is ASCII
        assert!(!cs.is_special(b'q' as u32));
        assert!(
            !cs.is_special(b'q' as u32),
            "upstream never clears SSflag in UTF-8"
        );
        cs.end_single_shift();
        assert!(cs.is_special(b'q' as u32));
    }

    #[test]
    fn decsc_saves_the_g_sets_but_not_the_single_shift() {
        let mut cs = Iso2022::new();
        cs.designate(0, CharSet::Special);
        let saved = cs.save();
        cs.designate(0, CharSet::Ascii);
        cs.invoke(Shift::Ss2);
        cs.restore(saved);
        assert!(
            cs.single_shift_pending(),
            "restore must not touch the single shift"
        );
        cs.end_single_shift();
        assert!(cs.is_special(b'q' as u32));
    }

    #[test]
    fn intermediates_select_g0_through_g3() {
        assert_eq!(gset_from_intermediate(b'('), 0);
        assert_eq!(gset_from_intermediate(b')'), 1);
        assert_eq!(gset_from_intermediate(b'*'), 2);
        assert_eq!(gset_from_intermediate(b'+'), 3);
    }

    #[test]
    fn katakana_needs_a_japanese_terminal() {
        assert_eq!(sbcs_final(b'I', false), None);
        assert_eq!(sbcs_final(b'I', true), Some(CharSet::Katakana));
        assert_eq!(sbcs_final(b'0', false), Some(CharSet::Special));
        assert_eq!(sbcs_final(b'B', false), Some(CharSet::Ascii));
    }
}
