//! The key table — `keyboard.c:GetKeyStr`.
//!
//! The other half of the frontend seam. Bytes *in* are the escape parser;
//! bytes *out* are this, and the core owns it for the same reason it owns the
//! mouse encoding: which form a key takes is terminal state set by escape
//! sequences the frontend never sees. `KEYBOARD.CNF` compatibility is the
//! other reason, and it is a compatibility artifact — a frontend that built
//! its own sequences would diverge the first time someone ported their
//! config.
//!
//! What the frontend supplies is a [`Key`], not a keysym: mapping a physical
//! key to one of these is platform work (Qt keysym on Linux, a scan code
//! through `KEYBOARD.CNF` on Windows) and belongs on the far side of the
//! boundary. Ordinary printable characters do not come through here at all.
//!
//! Verified against upstream rather than against ctlseqs: the oracle compiles
//! `keyboard.c` itself and a differential case sweeps every key in every mode
//! combination. See `oracle/README.md`.

/// A key with a terminal meaning. Names and grouping follow
/// `common/tttypes_key.h`, so a `KEYBOARD.CNF` reader can map onto them
/// directly.
///
/// `repr(u32)` is not decoration: the C ABI names these variants directly rather
/// than keeping a second copy of the list that can drift. See `tt-ffi`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Key {
    Up,
    Down,
    Right,
    Left,

    /// The numeric keypad. In numeric mode these send their printed
    /// character; in application-keypad mode, `SS3` plus a letter.
    Kp0,
    Kp1,
    Kp2,
    Kp3,
    Kp4,
    Kp5,
    Kp6,
    Kp7,
    Kp8,
    Kp9,
    KpMinus,
    KpComma,
    KpPeriod,
    KpSlash,
    KpAsterisk,
    KpPlus,
    KpEnter,

    /// DEC's four top-row keys. Always `SS3`, in every mode — they have no
    /// numeric form to fall back to.
    Pf1,
    Pf2,
    Pf3,
    Pf4,

    /// The VT220 editing keypad. On a PC keyboard these are Insert, Delete,
    /// Home, End, Page Up and Page Down.
    Find,
    Insert,
    Remove,
    Select,
    Prev,
    Next,

    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    /// F15 on a PC keyboard; DEC called it Help.
    Help,
    /// F16; DEC called it Do.
    Do,
    F17,
    F18,
    F19,
    F20,

    /// `IdXF1`…`IdXF5` — xterm's F1-F5, which DEC's numbering does not have
    /// because PF1-PF4 sat there.
    XF1,
    XF2,
    XF3,
    XF4,
    XF5,

    /// Local commands. They have key ids because `KEYBOARD.CNF` can bind
    /// them, but they put nothing on the wire.
    Hold,
    Print,
    Break,

    /// Shift+Tab.
    BackTab,
}

/// The modes a key's encoding depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct KeyModes {
    /// DECCKM, and not disabled by `ts.DisableAppCursor`.
    pub application_cursor: bool,
    /// DECNKM, and not disabled by `ts.DisableAppKeypad`.
    pub application_keypad: bool,
    /// `Send8BitMode` — S8C1T or DECSCL. Replaces `ESC [` with `9B` and
    /// `ESC O` with `8F`.
    pub eight_bit: bool,
    /// `cv.CRSend` — what a CR from the keyboard turns into. LNM rewrites it,
    /// and only keypad Enter in numeric mode reads it, that being the one key
    /// upstream marks `IdText` so it goes through newline conversion at all.
    pub cr_send: CrSend,
}

/// `ts.CRSend` / `cv.CRSend` — `ttcmn.c:OutControl` expands a CR by this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CrSend {
    /// `IdCR`, and `ttset.c:657`'s default.
    #[default]
    Cr,
    /// `IdCRLF`. `SM 20` (LNM) sets this.
    CrLf,
    /// `IdLF` — the CR is *replaced*, not extended.
    Lf,
}

/// `SS3` — the single shift that introduces the application forms.
fn ss3(modes: KeyModes) -> Vec<u8> {
    if modes.eight_bit {
        vec![0x8f]
    } else {
        vec![0x1b, b'O']
    }
}

/// `CSI`.
fn csi(modes: KeyModes) -> Vec<u8> {
    if modes.eight_bit {
        vec![0x9b]
    } else {
        vec![0x1b, b'[']
    }
}

fn seq(intro: Vec<u8>, tail: &[u8]) -> Vec<u8> {
    let mut v = intro;
    v.extend_from_slice(tail);
    v
}

impl Key {
    /// The bytes this key sends, or `None` when it sends nothing — which
    /// covers Hold, Print and Break, whose ids exist so `KEYBOARD.CNF` can
    /// bind them to local commands.
    pub fn encode(self, modes: KeyModes) -> Option<Vec<u8>> {
        use Key::*;

        // The cursor keys are the one group where application mode swaps the
        // *introducer* rather than the whole sequence.
        let cursor = |tail: u8| {
            Some(seq(
                if modes.application_cursor {
                    ss3(modes)
                } else {
                    csi(modes)
                },
                &[tail],
            ))
        };

        // A keypad key: its printed character in numeric mode, `SS3 <letter>`
        // in application mode.
        let keypad = |plain: u8, appli: u8| {
            if modes.application_keypad {
                Some(seq(ss3(modes), &[appli]))
            } else {
                Some(vec![plain])
            }
        };

        // The editing and function keys are `CSI <n> ~` regardless of mode.
        let tilde = |n: u8| Some(seq(csi(modes), format!("{n}~").as_bytes()));

        match self {
            Up => cursor(b'A'),
            Down => cursor(b'B'),
            Right => cursor(b'C'),
            Left => cursor(b'D'),

            Kp0 => keypad(b'0', b'p'),
            Kp1 => keypad(b'1', b'q'),
            Kp2 => keypad(b'2', b'r'),
            Kp3 => keypad(b'3', b's'),
            Kp4 => keypad(b'4', b't'),
            Kp5 => keypad(b'5', b'u'),
            Kp6 => keypad(b'6', b'v'),
            Kp7 => keypad(b'7', b'w'),
            Kp8 => keypad(b'8', b'x'),
            Kp9 => keypad(b'9', b'y'),
            KpMinus => keypad(b'-', b'm'),
            KpComma => keypad(b',', b'l'),
            KpPeriod => keypad(b'.', b'n'),
            KpSlash => keypad(b'/', b'o'),
            KpAsterisk => keypad(b'*', b'j'),
            KpPlus => keypad(b'+', b'k'),
            // The one key whose numeric form is *text* rather than a byte
            // string: upstream marks it `IdText` so it goes through newline
            // conversion, which is why LNM can turn it into CR LF.
            KpEnter => {
                if modes.application_keypad {
                    Some(seq(ss3(modes), b"M"))
                } else {
                    Some(match modes.cr_send {
                        CrSend::Cr => vec![0x0d],
                        CrSend::CrLf => vec![0x0d, 0x0a],
                        CrSend::Lf => vec![0x0a],
                    })
                }
            }

            // Never numeric. PF1-PF4 have no printed character to fall back
            // to, so they are `SS3` even with the keypad in numeric mode.
            Pf1 => Some(seq(ss3(modes), b"P")),
            Pf2 => Some(seq(ss3(modes), b"Q")),
            Pf3 => Some(seq(ss3(modes), b"R")),
            Pf4 => Some(seq(ss3(modes), b"S")),

            Find => tilde(1),
            Insert => tilde(2),
            Remove => tilde(3),
            Select => tilde(4),
            Prev => tilde(5),
            Next => tilde(6),

            // The gaps in this numbering are DEC's, not omissions: 16, 22, 27,
            // 30 and 35 were never assigned.
            F6 => tilde(17),
            F7 => tilde(18),
            F8 => tilde(19),
            F9 => tilde(20),
            F10 => tilde(21),
            F11 => tilde(23),
            F12 => tilde(24),
            F13 => tilde(25),
            F14 => tilde(26),
            Help => tilde(28),
            Do => tilde(29),
            F17 => tilde(31),
            F18 => tilde(32),
            F19 => tilde(33),
            F20 => tilde(34),

            XF1 => tilde(11),
            XF2 => tilde(12),
            XF3 => tilde(13),
            XF4 => tilde(14),
            XF5 => tilde(15),

            BackTab => Some(seq(csi(modes), b"Z")),

            Hold | Print | Break => None,
        }
    }

    /// Resolve a case-insensitive name, matching the oracle's `tt.key`
    /// directive so a differential case and a config file can use one
    /// spelling.
    pub fn parse(name: &str) -> Option<Key> {
        use Key::*;
        Some(match name.to_ascii_lowercase().as_str() {
            "up" => Up,
            "down" => Down,
            "right" => Right,
            "left" => Left,
            "kp0" => Kp0,
            "kp1" => Kp1,
            "kp2" => Kp2,
            "kp3" => Kp3,
            "kp4" => Kp4,
            "kp5" => Kp5,
            "kp6" => Kp6,
            "kp7" => Kp7,
            "kp8" => Kp8,
            "kp9" => Kp9,
            "kpminus" => KpMinus,
            "kpcomma" => KpComma,
            "kpperiod" => KpPeriod,
            "kpslash" => KpSlash,
            "kpasterisk" => KpAsterisk,
            "kpplus" => KpPlus,
            "kpenter" => KpEnter,
            "pf1" => Pf1,
            "pf2" => Pf2,
            "pf3" => Pf3,
            "pf4" => Pf4,
            "find" => Find,
            "insert" => Insert,
            "remove" => Remove,
            "select" => Select,
            "prev" => Prev,
            "next" => Next,
            "f6" => F6,
            "f7" => F7,
            "f8" => F8,
            "f9" => F9,
            "f10" => F10,
            "f11" => F11,
            "f12" => F12,
            "f13" => F13,
            "f14" => F14,
            "help" => Help,
            "do" => Do,
            "f17" => F17,
            "f18" => F18,
            "f19" => F19,
            "f20" => F20,
            "xf1" => XF1,
            "xf2" => XF2,
            "xf3" => XF3,
            "xf4" => XF4,
            "xf5" => XF5,
            "hold" => Hold,
            "print" => Print,
            "break" => Break,
            "backtab" => BackTab,
            _ => return None,
        })
    }

    /// Every key, for exhaustive tests.
    pub const ALL: &'static [Key] = &[
        Key::Up,
        Key::Down,
        Key::Right,
        Key::Left,
        Key::Kp0,
        Key::Kp1,
        Key::Kp2,
        Key::Kp3,
        Key::Kp4,
        Key::Kp5,
        Key::Kp6,
        Key::Kp7,
        Key::Kp8,
        Key::Kp9,
        Key::KpMinus,
        Key::KpComma,
        Key::KpPeriod,
        Key::KpSlash,
        Key::KpAsterisk,
        Key::KpPlus,
        Key::KpEnter,
        Key::Pf1,
        Key::Pf2,
        Key::Pf3,
        Key::Pf4,
        Key::Find,
        Key::Insert,
        Key::Remove,
        Key::Select,
        Key::Prev,
        Key::Next,
        Key::F6,
        Key::F7,
        Key::F8,
        Key::F9,
        Key::F10,
        Key::F11,
        Key::F12,
        Key::F13,
        Key::F14,
        Key::Help,
        Key::Do,
        Key::F17,
        Key::F18,
        Key::F19,
        Key::F20,
        Key::XF1,
        Key::XF2,
        Key::XF3,
        Key::XF4,
        Key::XF5,
        Key::Hold,
        Key::Print,
        Key::Break,
        Key::BackTab,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_cursor_mode_swaps_the_introducer_only() {
        let plain = KeyModes::default();
        let appli = KeyModes {
            application_cursor: true,
            ..plain
        };
        assert_eq!(Key::Up.encode(plain).unwrap(), b"\x1b[A");
        assert_eq!(Key::Up.encode(appli).unwrap(), b"\x1bOA");
    }

    #[test]
    fn the_keypad_is_numeric_until_told_otherwise() {
        let plain = KeyModes::default();
        assert_eq!(Key::Kp5.encode(plain).unwrap(), b"5");
        let appli = KeyModes {
            application_keypad: true,
            ..plain
        };
        assert_eq!(Key::Kp5.encode(appli).unwrap(), b"\x1bOu");
    }

    #[test]
    fn pf_keys_are_never_numeric() {
        // They have no printed character, so application-keypad mode makes no
        // difference to them — a plausible place to get the table wrong.
        let plain = KeyModes::default();
        assert_eq!(Key::Pf1.encode(plain).unwrap(), b"\x1bOP");
        assert_eq!(
            Key::Pf1
                .encode(KeyModes {
                    application_keypad: true,
                    ..plain
                })
                .unwrap(),
            b"\x1bOP"
        );
    }

    #[test]
    fn eight_bit_mode_replaces_both_introducers() {
        let eight = KeyModes {
            eight_bit: true,
            ..KeyModes::default()
        };
        assert_eq!(Key::Up.encode(eight).unwrap(), vec![0x9b, b'A']);
        assert_eq!(Key::Pf1.encode(eight).unwrap(), vec![0x8f, b'P']);
        assert_eq!(Key::F6.encode(eight).unwrap(), vec![0x9b, b'1', b'7', b'~']);
    }

    #[test]
    fn keypad_enter_is_the_one_key_newline_mode_reaches() {
        let plain = KeyModes::default();
        assert_eq!(Key::KpEnter.encode(plain).unwrap(), b"\r");
        assert_eq!(
            Key::KpEnter
                .encode(KeyModes {
                    cr_send: CrSend::CrLf,
                    ..plain
                })
                .unwrap(),
            b"\r\n"
        );
        // IdLF replaces the CR rather than extending it.
        assert_eq!(
            Key::KpEnter
                .encode(KeyModes {
                    cr_send: CrSend::Lf,
                    ..plain
                })
                .unwrap(),
            b"\n"
        );
        // ...and only in numeric mode. The application form is a fixed
        // sequence with no newline in it.
        assert_eq!(
            Key::KpEnter
                .encode(KeyModes {
                    cr_send: CrSend::CrLf,
                    application_keypad: true,
                    ..plain
                })
                .unwrap(),
            b"\x1bOM"
        );
    }

    #[test]
    fn local_commands_put_nothing_on_the_wire() {
        for k in [Key::Hold, Key::Print, Key::Break] {
            assert_eq!(k.encode(KeyModes::default()), None, "{k:?}");
        }
    }

    #[test]
    fn every_key_has_a_name_that_round_trips() {
        for &k in Key::ALL {
            let name = format!("{k:?}").to_ascii_lowercase();
            assert_eq!(Key::parse(&name), Some(k), "{k:?}");
        }
    }
}
