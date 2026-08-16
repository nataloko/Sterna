//! Find against a real grid: the part `find.rs`'s own unit tests cannot reach,
//! because it is about a buffer the host has been writing to rather than about
//! a pattern.
//!
//! Sibling of `tests/highlight.rs` and sharing its fixtures on purpose — the
//! wrapped line, the wide character and the scrolled-off line are the three
//! places where "a line" stops meaning "a row", and both features have to agree
//! about all three.

use tt_config::Settings;
use tt_session::find::{Hit, Query, Span};
use tt_session::Session;

fn session(cols: i32, rows: i32) -> Session {
    Session::from_settings(Settings {
        terminal_cols: cols,
        terminal_rows: rows,
        ..Settings::default()
    })
}

/// A plain-text search, which is what the bar ships with its boxes unticked.
fn text(pattern: &str) -> Query {
    Query {
        pattern: pattern.into(),
        literal: true,
        ..Query::default()
    }
}

fn looking_for(s: &mut Session, query: &Query) {
    s.set_find(Some(query)).expect("compiles");
}

/// `from`..`to` of every painted run on a row.
fn columns(spans: &[Span]) -> Vec<(u16, u16)> {
    spans.iter().map(|s| (s.from, s.to)).collect()
}

/// The whole of a hit, which is four numbers and reads better as one.
fn at(line: u64, from: u16, end_line: u64, to: u16) -> Hit {
    Hit {
        line,
        from,
        end_line,
        to,
    }
}

/// Step forward from the top of the buffer and collect everything, so a test
/// can assert the order as well as the set.
fn walk(s: &mut Session) -> Vec<Hit> {
    let mut out = Vec::new();
    let mut from = (0, 0);
    while let Some(hit) = s.find_next(from, false, false) {
        from = hit.after();
        out.push(hit);
    }
    out
}

#[test]
fn a_search_finds_what_is_on_the_page() {
    let mut s = session(40, 4);
    s.feed(b"an ERROR here");
    looking_for(&mut s, &text("ERROR"));

    assert_eq!(s.find_next((0, 0), false, false), Some(at(0, 3, 0, 8)));
    assert_eq!(s.find_count(), 1);
    assert_eq!(columns(s.row_find(0)), [(3, 8)]);
    assert!(s.row_find(1).is_empty(), "nothing on an empty row");
}

#[test]
fn nothing_is_searched_for_until_something_is_typed() {
    let mut s = session(40, 4);
    s.feed(b"an ERROR here");

    assert!(!s.has_find());
    assert_eq!(s.find_next((0, 0), false, false), None);
    assert_eq!(s.find_count(), 0);
    assert!(s.row_find(0).is_empty());

    // An empty field is somebody who has opened the bar and not typed. It is
    // not a pattern that matches everywhere, and it is not an error.
    s.set_find(Some(&Query::default())).expect("not an error");
    assert!(!s.has_find());
    assert!(s.row_find(0).is_empty());
}

#[test]
fn a_pattern_the_engine_refuses_leaves_the_last_one_running() {
    // Somebody typing `(ERROR)` passes through `(ERROR` on the way, and the
    // matches they are looking at must not blink out while they finish.
    let mut s = session(40, 4);
    s.feed(b"an ERROR here");
    looking_for(
        &mut s,
        &Query {
            pattern: "ERROR".into(),
            ..Query::default()
        },
    );
    let broken = Query {
        pattern: "(ERROR".into(),
        ..Query::default()
    };
    assert!(s.set_find(Some(&broken)).is_err());
    assert_eq!(columns(s.row_find(0)), [(3, 8)]);

    s.set_find(None).expect("clearing cannot fail");
    assert!(!s.has_find());
    assert!(s.row_find(0).is_empty());
}

#[test]
fn the_three_boxes_are_spellings_of_one_pattern() {
    let mut s = session(40, 4);
    s.feed(b"ERROR errors err");

    looking_for(&mut s, &text("err"));
    assert_eq!(s.find_count(), 2, "lower case only");

    looking_for(
        &mut s,
        &Query {
            ignore_case: true,
            ..text("err")
        },
    );
    assert_eq!(s.find_count(), 3);

    looking_for(
        &mut s,
        &Query {
            ignore_case: true,
            whole_word: true,
            ..text("err")
        },
    );
    assert_eq!(s.find_count(), 1, "`errors` is not the word `err`");
    assert_eq!(s.find_next((0, 0), false, false), Some(at(0, 13, 0, 16)));

    looking_for(
        &mut s,
        &Query {
            pattern: "err(or)?s".into(),
            ..Query::default()
        },
    );
    assert_eq!(s.find_count(), 1);
    assert_eq!(s.find_next((0, 0), false, false), Some(at(0, 6, 0, 12)));
}

#[test]
fn a_match_that_straddles_a_wrap_is_found_once_and_painted_on_both_rows() {
    // 10 columns, so "abcdefghijklmno" wraps after `j` and `hijkl` crosses the
    // join. A wrapped command is one line to whoever typed it, so a search that
    // walked rows would miss this entirely.
    let mut s = session(10, 4);
    s.feed(b"abcdefghijklmno");
    looking_for(&mut s, &text("hijkl"));

    assert_eq!(s.find_count(), 1, "one match, not one per row");
    assert_eq!(walk(&mut s), [at(0, 7, 1, 2)]);
    assert_eq!(columns(s.row_find(0)), [(7, 10)]);
    assert_eq!(columns(s.row_find(1)), [(0, 2)]);
}

#[test]
fn the_trailing_blanks_of_a_row_are_not_part_of_the_line() {
    // Every line in the grid is `cols` wide, so without the trim `$` would
    // anchor after 32 spaces and this would never match.
    let mut s = session(40, 4);
    s.feed(b"an ERROR");
    looking_for(
        &mut s,
        &Query {
            pattern: "ERROR$".into(),
            ..Query::default()
        },
    );
    assert_eq!(s.find_next((0, 0), false, false), Some(at(0, 3, 0, 8)));
}

#[test]
fn a_wide_character_is_matched_across_both_of_its_columns() {
    let mut s = session(20, 3);
    s.feed("[あ]".as_bytes());
    looking_for(&mut s, &text("あ"));
    // Column 1 is the glyph and column 2 is its padding cell; a selection made
    // from this hit has to take the pair.
    assert_eq!(s.find_next((0, 0), false, false), Some(at(0, 1, 0, 3)));
    assert_eq!(columns(s.row_find(0)), [(1, 3)]);
}

#[test]
fn the_scrollback_is_searched_and_scrolled_back_to() {
    let mut s = session(20, 2);
    s.feed(b"ERROR one\r\nplain\r\nplain\r\n");
    looking_for(&mut s, &text("ERROR"));

    let hit = s.find_next((0, 0), false, false).expect("in the history");
    assert_eq!(hit, at(0, 0, 0, 5));
    assert!(s.row_find(0).is_empty(), "scrolled off the live page");

    // What a frontend does with the answer: put the line back on screen.
    s.set_view_offset((s.top_line() - hit.line) as usize);
    assert_eq!(columns(s.row_find(0)), [(0, 5)]);
}

#[test]
fn a_line_that_has_aged_out_is_simply_not_found() {
    // The buffer's ends move as the host prints, which is why nothing is kept
    // between calls: a list of hits would have to say when it stopped being
    // true, and this says it by not being a list.
    let mut s = Session::from_settings(Settings {
        terminal_cols: 20,
        terminal_rows: 2,
        terminal_scrollback_lines: 3,
        ..Settings::default()
    });
    s.feed(b"ERROR one\r\n");
    looking_for(&mut s, &text("ERROR"));
    assert_eq!(s.find_count(), 1);

    for _ in 0..10 {
        s.feed(b"plain\r\n");
    }
    assert_eq!(s.find_count(), 0);
    assert_eq!(s.find_next((0, 0), true, true), None);
}

#[test]
fn stepping_visits_every_match_in_order_and_then_wraps() {
    let mut s = session(20, 4);
    s.feed(b"one hit\r\ntwo hit\r\nthree hit");
    looking_for(&mut s, &text("hit"));

    let all = walk(&mut s);
    assert_eq!(all, [at(0, 4, 0, 7), at(1, 4, 1, 7), at(2, 6, 2, 9)]);
    assert_eq!(s.find_count(), all.len());

    // Off the end without wrapping is nothing; with it, the first one again.
    let last = all[2];
    assert_eq!(s.find_next(last.after(), false, false), None);
    assert_eq!(s.find_next(last.after(), false, true), Some(all[0]));

    // And the mirror, backwards off the top.
    assert_eq!(s.find_next(all[0].before(), true, false), None);
    assert_eq!(s.find_next(all[0].before(), true, true), Some(last));
    assert_eq!(s.find_next(all[2].before(), true, false), Some(all[1]));
}

#[test]
fn wrapping_onto_the_only_match_there_is_stays_on_it() {
    // Next with one match must not report "no matches"; it has nowhere else to
    // go, and the honest answer is the match you are already looking at.
    let mut s = session(20, 3);
    s.feed(b"the only ERROR");
    looking_for(&mut s, &text("ERROR"));

    let hit = s.find_next((0, 0), false, false).expect("found");
    assert_eq!(s.find_next(hit.after(), false, true), Some(hit));
    assert_eq!(s.find_next(hit.before(), true, true), Some(hit));
}

#[test]
fn stepping_and_the_count_agree_about_overlapping_text() {
    // `aa` in `aaaa` is two matches to the engine. Stepping past the end of
    // each rather than past its start is what keeps the label and the button
    // telling the same story.
    let mut s = session(20, 3);
    s.feed(b"aaaa");
    looking_for(&mut s, &text("aa"));
    assert_eq!(s.find_count(), 2);
    assert_eq!(walk(&mut s), [at(0, 0, 0, 2), at(0, 2, 0, 4)]);
    assert_eq!(columns(s.row_find(0)), [(0, 4)], "one painted run");
}

#[test]
fn two_matches_on_one_row_are_two_painted_runs() {
    let mut s = session(20, 3);
    s.feed(b"hit and hit");
    looking_for(&mut s, &text("hit"));
    assert_eq!(columns(s.row_find(0)), [(0, 3), (8, 11)]);
}

#[test]
fn matches_follow_the_text_as_it_changes_under_them() {
    // The memo is keyed on the damage counter rather than on line content, so
    // this is the case that proves it is being retired.
    let mut s = Session::from_settings(Settings {
        terminal_cols: 20,
        terminal_rows: 3,
        terminal_cr_receive: tt_config::TerminalCrReceive::Cr,
        ..Settings::default()
    });
    looking_for(&mut s, &text("ERROR"));
    s.feed(b"an ERROR here");
    assert_eq!(columns(s.row_find(0)), [(3, 8)]);

    s.feed(b"\r");
    s.feed(b"no problem   ");
    assert!(s.row_find(0).is_empty(), "{:?}", s.row_find(0));
}

/// The one switch that must *not* reach this: `color.highlighting` is about the
/// user's own rules, and a find that painted nothing because a tick in the View
/// menu was off would be undiagnosable from the screen.
#[test]
fn the_highlight_switch_does_not_gate_a_search() {
    let mut s = Session::from_settings(Settings {
        terminal_cols: 40,
        terminal_rows: 3,
        color_highlighting: false,
        ..Settings::default()
    });
    s.feed(b"an ERROR here");
    looking_for(&mut s, &text("ERROR"));

    assert!(s.row_highlights(0).is_empty(), "the rules are off");
    assert_eq!(columns(s.row_find(0)), [(3, 8)]);
}

#[test]
fn a_search_and_the_rules_do_not_disturb_each_other() {
    // Two memos over one grid: painting a row for one must not throw away the
    // other's answer for it.
    let mut s = session(40, 3);
    s.feed(b"an ERROR here");
    s.set_highlights(&[tt_config::highlight::Rule {
        pattern: "here".into(),
        fore: Some([255, 0, 0]),
        ..tt_config::highlight::Rule::default()
    }]);
    looking_for(&mut s, &text("ERROR"));

    for _ in 0..3 {
        assert_eq!(columns(s.row_find(0)), [(3, 8)]);
        let rules: Vec<(u16, u16)> = s.row_highlights(0).iter().map(|s| (s.from, s.to)).collect();
        assert_eq!(rules, [(9, 13)]);
    }
}

#[test]
fn nothing_configured_costs_nothing() {
    let mut s = session(40, 24);
    s.feed(b"an ERROR here");
    for y in 0..24 {
        assert!(s.row_find(y).is_empty());
    }
}

/// A control mark is the terminal annotating its own screen, so the flatten
/// steps over it and a search sees the host's line whole. Without that, turning
/// a display switch on would silently stop a pattern matching — and the span
/// still covers both halves of the word, so the paint reaches around the mark.
#[test]
fn a_search_reads_through_a_control_mark() {
    let mut s = Session::from_settings(Settings {
        terminal_cols: 40,
        terminal_rows: 4,
        terminal_show_control_chars: true,
        ..Settings::default()
    });
    s.feed(b"an ERR\x07OR here");
    looking_for(&mut s, &text("ERROR"));

    assert_eq!(s.find_count(), 1);
    // One match, painted as two runs: `ERR` at 3..6 and `OR` at 8..10, with
    // the mark's own two columns left alone in between. The highlight reaches
    // around the annotation rather than over it, which is what the mark being
    // outside the text means when the text is painted.
    assert_eq!(columns(s.row_find(0)), [(3, 6), (8, 10)]);
}
