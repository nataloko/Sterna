//! Highlight rules against a real grid: the part `highlight.rs`'s own unit
//! tests cannot reach, because it is about cells rather than about text.

use tt_config::highlight::{Rule, Scope, STYLE_BOLD};
use tt_config::Settings;
use tt_session::highlight::Span;
use tt_session::Session;

fn red(pattern: &str) -> Rule {
    Rule {
        pattern: pattern.into(),
        fore: Some([255, 0, 0]),
        ..Rule::default()
    }
}

fn session(cols: i32, rows: i32) -> Session {
    Session::from_settings(Settings {
        terminal_cols: cols,
        terminal_rows: rows,
        ..Settings::default()
    })
}

/// `from`..`to` of every span on a row, which is what most of these assert.
fn columns(spans: &[Span]) -> Vec<(u16, u16)> {
    spans.iter().map(|s| (s.from, s.to)).collect()
}

#[test]
fn a_rule_colours_the_columns_its_pattern_matched() {
    let mut s = session(40, 4);
    s.feed(b"an ERROR here");
    s.set_highlights(&[red("ERROR")]);

    assert_eq!(columns(s.row_highlights(0)), [(3, 8)]);
    assert_eq!(s.row_highlights(0)[0].fg, Some([255, 0, 0]));
    // Nothing on the rows that have nothing on them.
    assert!(s.row_highlights(1).is_empty());
}

#[test]
fn the_trailing_blanks_of_a_row_are_not_part_of_the_line() {
    // Every line in the grid is `cols` wide, so without the trim `$` would
    // anchor after 27 spaces and a rule ending in one would never match.
    let mut s = session(40, 4);
    s.feed(b"an ERROR");
    s.set_highlights(&[red("ERROR$")]);
    assert_eq!(columns(s.row_highlights(0)), [(3, 8)]);
}

#[test]
fn a_line_scope_stops_at_the_text_and_not_at_the_margin() {
    let mut s = session(40, 4);
    s.feed(b"an ERROR");
    let mut rule = red("ERROR");
    rule.scope = Scope::Line;
    s.set_highlights(&[rule]);
    assert_eq!(columns(s.row_highlights(0)), [(0, 8)]);
}

#[test]
fn a_match_that_straddles_a_wrap_is_coloured_on_both_rows() {
    // 10 columns, so "abcdefghijklmno" wraps after `j` and `fghijklmn`
    // crosses the join. A wrapped command is one line to whoever typed it.
    let mut s = session(10, 4);
    s.feed(b"abcdefghijklmno");
    s.set_highlights(&[red("hijkl")]);

    assert_eq!(columns(s.row_highlights(0)), [(7, 10)]);
    assert_eq!(columns(s.row_highlights(1)), [(0, 2)]);
}

#[test]
fn a_wide_character_is_coloured_across_both_of_its_columns() {
    let mut s = session(20, 3);
    s.feed("[あ]".as_bytes());
    s.set_highlights(&[red("あ")]);
    // Column 1 is the glyph and column 2 is its padding cell; the painter
    // draws the pair as one two-column run, and the span has to cover it.
    assert_eq!(columns(s.row_highlights(0)), [(1, 3)]);
}

#[test]
fn the_scrollback_is_coloured_by_a_rule_written_afterwards() {
    // The reason matching happens while painting rather than while receiving:
    // a rule applies to what is already on the screen, history included.
    let mut s = session(20, 2);
    s.feed(b"ERROR one\r\nplain\r\nplain\r\n");
    s.set_highlights(&[red("ERROR")]);

    assert!(s.row_highlights(0).is_empty(), "scrolled off the live page");
    s.set_view_offset(2);
    assert_eq!(columns(s.row_highlights(0)), [(0, 5)]);
}

#[test]
fn the_switch_and_the_rules_own_switch_both_stop_it() {
    let mut settings = Settings {
        terminal_cols: 40,
        terminal_rows: 3,
        ..Settings::default()
    };
    let mut s = Session::from_settings(settings.clone());
    s.feed(b"an ERROR here");
    s.set_highlights(&[red("ERROR")]);
    assert_eq!(columns(s.row_highlights(0)), [(3, 8)]);

    settings.color_highlighting = false;
    s.set_settings(settings.clone());
    assert!(s.row_highlights(0).is_empty(), "the master switch");

    settings.color_highlighting = true;
    s.set_settings(settings);
    let mut off = red("ERROR");
    off.enabled = false;
    s.set_highlights(&[off]);
    assert!(s.row_highlights(0).is_empty(), "the rule's own switch");
}

#[test]
fn a_rule_follows_the_text_as_it_changes_under_it() {
    // The memo is keyed on a damage counter rather than on line content, so
    // this is the case that proves it is being retired.
    let mut s = session(20, 3);
    s.set_highlights(&[red("ERROR")]);
    s.feed(b"an ERROR here");
    assert_eq!(columns(s.row_highlights(0)), [(3, 8)]);

    s.feed(b"\r");
    s.feed(b"no problem   ");
    assert!(s.row_highlights(0).is_empty(), "{:?}", s.row_highlights(0));
}

#[test]
fn attributes_travel_without_a_colour() {
    let mut s = session(20, 3);
    s.feed(b"an ERROR here");
    s.set_highlights(&[Rule {
        pattern: "ERROR".into(),
        style: STYLE_BOLD,
        ..Rule::default()
    }]);
    let spans = s.row_highlights(0);
    assert_eq!(columns(spans), [(3, 8)]);
    assert_eq!(spans[0].fg, None);
    assert_eq!(spans[0].bg, None);
    assert_eq!(spans[0].attrs, tt_grid::ATTR_BOLD);
}

#[test]
fn a_rule_that_will_not_compile_is_reported_and_the_rest_still_work() {
    let mut s = session(20, 3);
    s.feed(b"an ERROR here");
    s.set_highlights(&[
        Rule {
            label: "broken".into(),
            pattern: "(unclosed".into(),
            fore: Some([1, 2, 3]),
            ..Rule::default()
        },
        red("ERROR"),
    ]);
    assert_eq!(s.highlight_rejected().len(), 1);
    assert_eq!(s.highlight_rejected()[0].index, 1);
    assert_eq!(s.highlight_rejected()[0].label, "broken");
    assert_eq!(columns(s.row_highlights(0)), [(3, 8)]);
}

#[test]
fn nothing_configured_costs_nothing() {
    let mut s = session(40, 24);
    s.feed(b"an ERROR here");
    for y in 0..24 {
        assert!(s.row_highlights(y).is_empty());
    }
    assert!(s.highlight_rejected().is_empty());
}
