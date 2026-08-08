//! Property tests over generated escape-sequence streams.
//!
//! These overlap the fuzzer and are not redundant with it. libFuzzer mutates
//! bytes and needs nightly, a corpus and minutes; proptest generates
//! *sequences* — a CSI with plausible parameters, a wide character, a scroll
//! region — so it reaches a grid with a left margin and a wrapped double-width
//! glyph in a handful of steps rather than by accident. It also shrinks, so a
//! failure arrives as the three sequences that caused it rather than as 400
//! bytes of noise. And it runs on stable, which is what puts it in CI.
//!
//! The invariants are `PLAN.md`'s list: cursor in bounds and wide pairs never
//! split (both in [`tt_grid::Grid::check_invariants`]), the scrollback never
//! rewriting history, and no attribute leaking through an erase.

use proptest::prelude::*;
use tt_fuzz::{chunk, config, vt_chunking, vt_stream, vt_wide_pairs};
use tt_grid::{ATTR2_COLOR_MASK, ATTR_SGR_MASK};
use tt_vt::Vt;

/// One step of a generated stream.
///
/// The weights matter more than the list does: text has to dominate or the
/// grid stays empty and every operation acts on blanks, which is precisely the
/// state where a wide-character bug cannot show.
fn step() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        6 => text(),
        4 => csi(),
        2 => private_mode(),
        2 => sgr(),
        2 => control(),
        2 => escape(),
        1 => rectangle(),
        1 => osc(),
    ]
}

/// Narrow, wide and combining characters in one generator, because the
/// interesting cases are the ones where they meet — a combining mark landing on
/// a padding cell, a double-width glyph one column from the margin.
fn text() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(
        prop_oneof![
            8 => 0x20u32..0x7f,            // ASCII
            2 => Just(0x4e00u32),          // 一, double width
            2 => Just(0xff21u32),          // Ａ, a fullwidth form
            2 => Just(0x0301u32),          // combining acute
            1 => Just(0x00a0u32),          // NBSP
            1 => Just(0x1f600u32),         // an emoji, four UTF-8 bytes
        ],
        1..12,
    )
    .prop_map(|cps| {
        let mut out = Vec::new();
        for cp in cps {
            let mut buf = [0u8; 4];
            out.extend_from_slice(
                char::from_u32(cp)
                    .unwrap_or('?')
                    .encode_utf8(&mut buf)
                    .as_bytes(),
            );
        }
        out
    })
}

/// `CSI Ps ; Ps <final>`, over the finals that move, erase, insert, delete,
/// scroll, set the margins or ask a question.
fn csi() -> impl Strategy<Value = Vec<u8>> {
    let finals = "ABCDEFGHIJKLMPSTXZ@dabejk`rsucfghlmnt";
    (
        prop::sample::select(finals.chars().collect::<Vec<_>>()),
        prop::option::of(0u16..30),
        prop::option::of(0u16..30),
    )
        .prop_map(|(f, a, b)| {
            let mut s = String::from("\x1b[");
            if let Some(a) = a {
                s.push_str(&a.to_string());
            }
            if let Some(b) = b {
                s.push(';');
                s.push_str(&b.to_string());
            }
            s.push(f);
            s.into_bytes()
        })
}

/// The private modes, which is where the alternate screen, autowrap, origin
/// mode and every mouse tracking mode live — and where the alt-screen restore
/// bug that started this file was reachable from.
fn private_mode() -> impl Strategy<Value = Vec<u8>> {
    (
        prop::sample::select(vec![
            1u16, 3, 5, 6, 7, 9, 12, 25, 47, 66, 69, 1000, 1002, 1003, 1004, 1005, 1006, 1015,
            1016, 1047, 1048, 1049, 2004, 7786,
        ]),
        any::<bool>(),
    )
        .prop_map(|(n, set)| format!("\x1b[?{n}{}", if set { 'h' } else { 'l' }).into_bytes())
}

fn sgr() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        (0u16..30).prop_map(|n| format!("\x1b[{n}m")),
        (0u16..255).prop_map(|n| format!("\x1b[38;5;{n}m")),
        (0u16..255).prop_map(|n| format!("\x1b[48;5;{n}m")),
        (0u16..255, 0u16..255).prop_map(|(r, g)| format!("\x1b[38;2;{r};{g};0m")),
        Just("\x1b[m".to_string()),
    ]
    .prop_map(String::into_bytes)
}

fn control() -> impl Strategy<Value = Vec<u8>> {
    prop::sample::select(vec![
        b"\r".to_vec(),
        b"\n".to_vec(),
        b"\x08".to_vec(),
        b"\t".to_vec(),
        b"\x0b".to_vec(),
        b"\x0c".to_vec(),
        b"\x0e".to_vec(), // SO
        b"\x0f".to_vec(), // SI
        b"\x7f".to_vec(),
    ])
}

fn escape() -> impl Strategy<Value = Vec<u8>> {
    prop::sample::select(vec![
        b"\x1b7".to_vec(),
        b"\x1b8".to_vec(),
        b"\x1bD".to_vec(),
        b"\x1bE".to_vec(),
        b"\x1bM".to_vec(),
        b"\x1bH".to_vec(),
        b"\x1bZ".to_vec(),
        b"\x1bc".to_vec(),
        b"\x1b#8".to_vec(),
        b"\x1b(0".to_vec(),
        b"\x1b(B".to_vec(),
        b"\x1b)0".to_vec(),
        b"\x1bN".to_vec(),
        b"\x1bO".to_vec(),
        b"\x1b~".to_vec(),
        b"\x1b}".to_vec(),
    ])
}

/// The `$`-intermediate family — DECCARA, DECFRA, DECERA, DECCRA and friends.
/// They take four coordinates each and clamp them against the margins, which
/// is the arithmetic most likely to walk off a line.
fn rectangle() -> impl Strategy<Value = Vec<u8>> {
    (
        prop::sample::select(vec!["$r", "$t", "$x", "$z", "${", "$v"]),
        prop::collection::vec(0u16..12, 0..6),
    )
        .prop_map(|(op, params)| {
            let list: Vec<String> = params.iter().map(|p| p.to_string()).collect();
            format!("\x1b[{}{op}", list.join(";")).into_bytes()
        })
}

fn osc() -> impl Strategy<Value = Vec<u8>> {
    ("[a-z ]{0,8}", prop::sample::select(vec![0u8, 1, 2]))
        .prop_map(|(s, n)| format!("\x1b]{n};{s}\x07").into_bytes())
}

fn stream() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(step(), 1..40).prop_map(|steps| steps.concat())
}

/// The streams over which wide-character pairing is *ours* to keep.
///
/// Narrower than it looks, and every exclusion is a place Tera Term breaks the
/// pairing itself — see `Grid::check_wide_pairs`. Three of them:
///
/// - **the rectangular family** (`$ v` and friends), because DECCRA is a bare
///   `memcpyW` with no fixup anywhere in it or its caller;
/// - **the alternate screen** (`? 1047`/`1048`/`1049`), because the restore
///   clips to `min(saved, current)` columns and can leave the destination's own
///   padding behind;
/// - **insert mode** (`SM 4`), because the shift for a *double-width* insert
///   moves the cell two places and upstream only crushes the one at
///   `LineEnd - 1`, so a lead two cells in still arrives at the margin alone.
///
/// So `h` and `l` go with the rest, which costs this property the mode toggles.
/// They are not lost: the full generator still drives them through
/// [`generated_streams_keep_the_grid_consistent`], which asserts the structural
/// invariants rather than the pairing.
fn stream_where_the_pairing_is_ours() -> impl Strategy<Value = Vec<u8>> {
    let finals = "ABCDEFGIJKLMPSTXZ@dabejk`rsucfmn";
    let csi_no_modes = (
        prop::sample::select(finals.chars().collect::<Vec<_>>()),
        prop::option::of(0u16..30),
        prop::option::of(0u16..30),
    )
        .prop_map(|(f, a, b)| {
            let mut s = String::from("\x1b[");
            if let Some(a) = a {
                s.push_str(&a.to_string());
            }
            if let Some(b) = b {
                s.push(';');
                s.push_str(&b.to_string());
            }
            s.push(f);
            s.into_bytes()
        });
    prop::collection::vec(
        prop_oneof![
            8 => text(),
            4 => csi_no_modes,
            3 => control(),
            3 => escape(),
            2 => sgr(),
        ],
        1..40,
    )
    .prop_map(|steps| steps.concat())
}

/// And without a resize, which is the one thing that legitimately *does*
/// rewrite history: `Grid::resize` refits every retained line to the new width
/// and hands lines back out of the scrollback to fill a taller page, so a line
/// at a given absolute index is genuinely not the line it was. Upstream's
/// `ChangeBuffer` does the same. `Session::resize` and `Session::follow_scroll`
/// both answer it by going live rather than by pretending the anchor survived.
fn stream_without_resize() -> impl Strategy<Value = Vec<u8>> {
    let finals = "ABCDEFGHIJKLMPSTXZ@dabejk`rsucfghlmn";
    let csi_no_t = (
        prop::sample::select(finals.chars().collect::<Vec<_>>()),
        prop::option::of(0u16..30),
        prop::option::of(0u16..30),
    )
        .prop_map(|(f, a, b)| {
            let mut s = String::from("\x1b[");
            if let Some(a) = a {
                s.push_str(&a.to_string());
            }
            if let Some(b) = b {
                s.push(';');
                s.push_str(&b.to_string());
            }
            s.push(f);
            s.into_bytes()
        });
    prop::collection::vec(
        prop_oneof![
            8 => text(),
            4 => csi_no_t,
            3 => control(),
            3 => escape(),
            2 => sgr(),
        ],
        1..40,
    )
    .prop_map(|steps| steps.concat())
}

proptest! {
    /// `PLAN.md`'s "cursor in bounds, wide-char pairs never split", plus the
    /// rest of the structural contract, after every chunk of every stream.
    #[test]
    fn generated_streams_keep_the_grid_consistent(bytes in stream()) {
        prop_assert!(vt_stream(config(), &chunk(&bytes, &[1])).is_ok(),
            "{:?}", vt_stream(config(), &chunk(&bytes, &[1])));
    }

    /// The chunking property, on streams built out of whole sequences — so the
    /// boundary lands inside one about as often as not.
    #[test]
    fn generated_streams_do_not_care_where_the_chunks_fall(
        bytes in stream(),
        sizes in prop::collection::vec(1usize..9, 1..4),
    ) {
        let chunks = chunk(&bytes, &sizes);
        prop_assert!(vt_chunking(config(), &chunks).is_ok(),
            "{:?}", vt_chunking(config(), &chunks));
    }

    /// `PLAN.md`'s "wide-char pairs never split", over the paths that own it.
    ///
    /// The paths upstream breaks the pairing on are excluded from the generator
    /// rather than from the assertion, because the port reproduces upstream
    /// there — see [`stream_where_the_pairing_is_ours`] for the list and the
    /// citation for each. What is left is writing, wrapping, deleting,
    /// scrolling and erasing, all of which must leave both halves of a wide
    /// character together or neither.
    ///
    /// It has earned its keep twice: the parked space that broke the wide cell
    /// under it (`buffer.c:3219`, and the oracle could see that one), and the
    /// insert shift that pushed a lead to the margin without its padding
    /// (`buffer.c:3298`, which the oracle could not see at all).
    #[test]
    fn the_text_paths_never_leave_half_a_wide_character(
        bytes in stream_where_the_pairing_is_ours(),
    ) {
        let chunks = chunk(&bytes, &[1]);
        prop_assert!(vt_wide_pairs(config(), &chunks).is_ok(),
            "{:?}", vt_wide_pairs(config(), &chunks));
    }

    /// `PLAN.md`'s "no attribute leaks across BCE".
    ///
    /// Erasing paints the *pen's colours* over a cell but never its SGR bits —
    /// `buffer.c` passes `CurCharAttr.Fore`/`Back` with `AttrDefault`, which is
    /// the asymmetry `Cell::erased` encodes. Getting it backwards is invisible
    /// until something colours a background and then clears part of it, so it
    /// is asserted over the whole page after an unconditional `ED 2`.
    #[test]
    fn an_erase_never_leaves_an_sgr_attribute_behind(bytes in stream()) {
        let mut vt = Vt::new(config());
        vt.feed(&bytes);
        vt.feed(b"\x1b[2J");

        let grid = vt.grid();
        let pen = grid.pen;
        for y in 0..grid.rows() {
            for (x, cell) in grid.line(y).iter().enumerate() {
                prop_assert_eq!(cell.attrs & ATTR_SGR_MASK, 0,
                    "cell {},{} kept an SGR bit through ED 2", x, y);
                prop_assert_eq!(cell.attrs & ATTR2_COLOR_MASK, pen.attrs & ATTR2_COLOR_MASK,
                    "cell {},{} lost the pen's colour flags", x, y);
                prop_assert_eq!(cell.fg, pen.fg, "cell {},{} foreground", x, y);
                prop_assert_eq!(cell.bg, pen.bg, "cell {},{} background", x, y);
            }
        }
    }

    /// The scrollback is history, and history does not get rewritten.
    ///
    /// This is the property the viewport in `tt-session` rests on: it anchors
    /// to *content* by counting `scrolled_off`, which is only meaningful if the
    /// line at a given absolute index stays the line it was. Asserted by
    /// recording every scrollback line against its absolute index as the stream
    /// is fed, and failing if an index ever comes back holding something else.
    ///
    /// `scrolled_off` itself must be monotonic for the same reason — the
    /// session moves the offset by the *difference* between two readings, so a
    /// decrease would move the view the wrong way.
    #[test]
    fn the_scrollback_never_rewrites_a_line_it_has_already_kept(bytes in stream_without_resize()) {
        let mut vt = Vt::new(config());
        let mut seen: std::collections::HashMap<u64, Vec<tt_grid::Cell>> = Default::default();
        let mut last_off = 0;

        for piece in chunk(&bytes, &[3]) {
            vt.feed(piece);
            let grid = vt.grid();

            prop_assert!(grid.scrolled_off() >= last_off,
                "scrolled_off went {} -> {}", last_off, grid.scrolled_off());
            last_off = grid.scrolled_off();

            // Absolute index of the oldest line still retained.
            let base = grid.scrolled_off() - grid.scrollback_len() as u64;
            for i in 0..grid.scrollback_len() {
                let line = grid.scrollback_line(i).expect("in range").to_vec();
                if let Some(before) = seen.get(&(base + i as u64)) {
                    prop_assert_eq!(before, &line,
                        "scrollback line {} was rewritten", base + i as u64);
                } else {
                    seen.insert(base + i as u64, line);
                }
            }
        }
    }
}
