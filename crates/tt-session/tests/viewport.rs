//! Scrolling back through the history.
//!
//! The viewport lives here rather than in the frontend for one reason: it has
//! to be reconciled every time the grid scrolls, and only the thing that owns
//! the feed knows when that happened. A frontend holding "n lines from the
//! bottom" watches what it is reading walk up the screen as the host talks.

use tt_session::{MemoryHandle, MemoryTransport, Session};
use tt_vt::Config;

fn session(cols: usize, rows: usize, scrollback: usize) -> (Session, MemoryHandle) {
    let mut s = Session::new(Config {
        cols,
        rows,
        scrollback_max: scrollback,
        ..Config::default()
    });
    let (transport, handle) = MemoryTransport::new();
    s.connect(Box::new(transport));
    (s, handle)
}

fn row(s: &Session, y: usize) -> String {
    let mut out = String::new();
    for cell in s.row(y) {
        match cell.codepoints().next() {
            Some(0) | None => out.push(' '),
            Some(cp) => out.push(char::from_u32(cp).unwrap_or(' ')),
        }
    }
    out.trim_end().to_string()
}

/// `n` numbered lines, each ending in CR LF so the grid scrolls.
fn lines(from: usize, to: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for i in from..to {
        out.extend_from_slice(format!("line{i}\r\n").as_bytes());
    }
    out
}

#[test]
fn nothing_is_scrolled_back_until_something_scrolls_back() {
    let (mut s, _h) = session(20, 4, 100);
    s.feed(&lines(0, 3));
    assert_eq!(s.scrollback_len(), 0, "three lines fit in four rows");
    assert_eq!(s.view_offset(), 0);
    assert_eq!(row(&s, 0), "line0");
}

#[test]
fn scrolling_back_shows_the_history() {
    let (mut s, _h) = session(20, 4, 100);
    s.feed(&lines(0, 10));
    // Ten lines through a four-row screen leaves seven behind: the tenth
    // newline puts the cursor on an empty row 3.
    assert_eq!(s.scrollback_len(), 7);
    assert_eq!(row(&s, 0), "line7");

    s.set_view_offset(3);
    assert_eq!(row(&s, 0), "line4");
    assert_eq!(row(&s, 3), "line7");

    s.set_view_offset(0);
    assert_eq!(row(&s, 0), "line7");
}

#[test]
fn a_scrolled_back_view_holds_still_while_the_host_talks() {
    // The claim the whole design rests on. A view anchored to the bottom
    // slides by one for every line printed, so reading a boot log on a device
    // that is still booting becomes impossible — which is exactly when anyone
    // scrolls back on a serial console.
    let (mut s, _h) = session(20, 4, 100);
    s.feed(&lines(0, 10));
    s.set_view_offset(5);
    assert_eq!(row(&s, 0), "line2");

    s.feed(&lines(10, 15));
    assert_eq!(row(&s, 0), "line2", "the view moved under the reader");
    assert_eq!(s.view_offset(), 10, "counted from a bottom that moved");
}

#[test]
fn holding_still_survives_the_scrollback_filling_up_and_evicting() {
    // Once the buffer is full its length stops changing, so a viewport that
    // reconciled against the length would think nothing had moved — while
    // every line pushed silently drops the oldest and shifts everything.
    let (mut s, _h) = session(20, 4, 8);
    s.feed(&lines(0, 12));
    assert_eq!(s.scrollback_len(), 8, "full");

    s.set_view_offset(4);
    let held = row(&s, 0);
    s.feed(&lines(12, 15));
    assert_eq!(s.scrollback_len(), 8, "still full, so length says nothing");
    assert_eq!(row(&s, 0), held);
}

#[test]
fn a_view_pushed_off_the_top_lands_on_the_oldest_line() {
    let (mut s, _h) = session(20, 4, 5);
    s.feed(&lines(0, 10));
    s.set_view_offset(5);
    let oldest = row(&s, 0);

    // Enough output to push the view past the start of the history. It has to
    // stop at the top rather than wrap, and rather than snapping back to live
    // — being dumped at the bottom mid-read is the thing being avoided.
    s.feed(&lines(10, 40));
    assert_eq!(s.view_offset(), 5, "clamped to the history that exists");
    assert_ne!(row(&s, 0), oldest, "the content itself was evicted");
}

#[test]
fn the_view_cannot_be_set_past_the_history() {
    let (mut s, _h) = session(20, 4, 100);
    s.feed(&lines(0, 10));
    s.set_view_offset(9999);
    assert_eq!(s.view_offset(), 7);
    assert_eq!(row(&s, 0), "line0");
}

#[test]
fn dropping_the_scrollback_drops_the_view_with_it() {
    let (mut s, _h) = session(20, 4, 100);
    s.feed(&lines(0, 10));
    s.set_view_offset(5);

    // `ED 3` is `ClearBuffer`, which discards the history outright. A view
    // still claiming to be five lines up would be reading freed history.
    s.feed(b"\x1b[3J");
    assert_eq!(s.scrollback_len(), 0);
    assert_eq!(s.view_offset(), 0);
}

#[test]
fn a_resize_goes_live() {
    // Resizing moves lines between the page and the scrollback in both
    // directions — growing pulls them back out — so whatever the view was
    // anchored to has moved by an amount that is not the scroll count. Going
    // live is the honest answer.
    let (mut s, _h) = session(20, 4, 100);
    s.feed(&lines(0, 10));
    s.set_view_offset(5);
    s.resize(20, 8).unwrap();
    assert_eq!(s.view_offset(), 0);
}

/// And a resize that nobody asked for goes live too.
///
/// `Session::resize` is the frontend's path. DECCOLM and the XTWINOPS resize
/// reach the same `Grid::resize` from inside the parser, so the anchor moves
/// with no call to reconcile it — the view would be left counting from a
/// scrollback that had just gained or lost lines.
#[test]
fn a_resize_from_the_far_end_goes_live_as_well() {
    for resize in [&b"\x1b[8;8;20t"[..], &b"\x1b[?3h"[..], &b"\x1b[?3l"[..]] {
        let (mut s, _h) = session(20, 4, 100);
        s.feed(&lines(0, 10));
        s.set_view_offset(5);
        assert_eq!(s.view_offset(), 5);

        s.feed(resize);
        assert_eq!(s.view_offset(), 0, "after {resize:?}");
    }
}

#[test]
fn the_cursor_is_reported_only_while_it_is_on_screen() {
    let (mut s, _h) = session(20, 4, 100);
    s.feed(&lines(0, 10));
    // Live: the cursor is on the last row, having just had a line feed.
    assert_eq!(s.cursor_view_row(), Some(3));

    // Scrolled back by one, it slides down one and off the bottom. A window
    // that painted it anyway would show a cursor sitting on a line of history
    // it does not belong to.
    s.set_view_offset(1);
    assert_eq!(s.cursor_view_row(), None);
}

#[test]
fn a_terminal_with_no_scrollback_configured_has_no_viewport() {
    let (mut s, _h) = session(20, 4, 0);
    s.feed(&lines(0, 20));
    assert_eq!(s.scrollback_len(), 0);
    s.set_view_offset(10);
    assert_eq!(s.view_offset(), 0);
    assert_eq!(row(&s, 3), "");
}

// --- naming a line so it can be kept ----------------------------------------
//
// Everything above is about where a line is *now*. A frontend that wants to
// hold on to one — a selection is the only thing that does — needs a name that
// survives the host printing another screenful, and neither a viewport row nor
// a grid row is one.

fn line(s: &Session, n: u64) -> Option<String> {
    s.line(n).map(|cells| {
        let mut out = String::new();
        for cell in cells {
            match cell.codepoints().next() {
                Some(0) | None => out.push(' '),
                Some(cp) => out.push(char::from_u32(cp).unwrap_or(' ')),
            }
        }
        out.trim_end().to_string()
    })
}

#[test]
fn a_lines_number_does_not_change_when_the_host_talks() {
    let (mut s, _h) = session(20, 4, 100);
    s.feed(&lines(0, 10));
    // Whatever "line3" is called now, it is still called that afterwards —
    // which is the entire property, and the one a viewport row does not have.
    let n = s.line_at(0);
    assert_eq!(line(&s, n).as_deref(), Some("line7"));
    let row0_before = row(&s, 0);

    s.feed(&lines(10, 30));
    assert_eq!(line(&s, n).as_deref(), Some("line7"), "same line, same number");
    assert_ne!(row(&s, 0), row0_before, "...while viewport row 0 moved on");
}

#[test]
fn the_top_of_the_page_is_the_origin() {
    let (mut s, _h) = session(20, 4, 100);
    assert_eq!(s.top_line(), 0, "nothing has left the page yet");
    assert_eq!(s.line_at(0), 0);

    s.feed(&lines(0, 10));
    // Seven lines left the page, so the top of it is line seven.
    assert_eq!(s.top_line(), 7);
    assert_eq!(line(&s, 7).as_deref(), Some("line7"));
    assert_eq!(line(&s, 0).as_deref(), Some("line0"), "the oldest still held");
}

#[test]
fn scrolling_back_renumbers_nothing() {
    let (mut s, _h) = session(20, 4, 100);
    s.feed(&lines(0, 10));
    s.set_view_offset(5);
    // Viewport row 0 now shows line2, and `line_at` says so — the offset moves
    // which line a row shows, not what that line is called.
    assert_eq!(s.line_at(0), 2);
    assert_eq!(row(&s, 0), "line2");
    assert_eq!(line(&s, 2).as_deref(), Some("line2"));
}

#[test]
fn a_line_that_has_been_evicted_is_gone_rather_than_wrong() {
    // Three lines of history, so the fourth pushes the first out.
    let (mut s, _h) = session(20, 4, 3);
    s.feed(&lines(0, 12));
    assert_eq!(s.scrollback_len(), 3);
    assert_eq!(line(&s, 0), None, "evicted, and says so");
    assert_eq!(line(&s, u64::MAX), None, "not printed yet either");
    // The oldest line still held is one scrollback-worth above the page.
    let oldest = s.top_line() - 3;
    assert_eq!(line(&s, oldest).as_deref(), Some("line6"));
}

#[test]
fn dropping_the_scrollback_leaves_the_page_numbered_as_it_was() {
    let (mut s, _h) = session(20, 4, 100);
    s.feed(&lines(0, 10));
    let top = s.line_at(0);
    // `ED 3` is `ClearBuffer`: the history goes and the page is blanked, so
    // the numbering has to survive `scrollback_len` collapsing underneath it.
    // The page keeps its numbers — those rows are still there, just empty —
    // and everything above them is unreachable rather than misread.
    s.feed(b"\x1b[3J");
    assert_eq!(s.scrollback_len(), 0);
    assert_eq!(line(&s, top).as_deref(), Some(""));
    assert_eq!(line(&s, top - 1), None);
    assert_eq!(line(&s, top + 3).as_deref(), Some(""), "the last page row");
    assert_eq!(line(&s, top + 4), None, "and one past it");
}

#[test]
fn every_viewport_row_is_the_line_it_is_named_after() {
    let (mut s, _h) = session(20, 6, 100);
    s.feed(&lines(0, 40));
    for offset in [0, 1, 7, 30] {
        s.set_view_offset(offset);
        for y in 0..6 {
            assert_eq!(
                line(&s, s.line_at(y)).as_deref(),
                Some(row(&s, y).as_str()),
                "offset {offset}, row {y}"
            );
        }
    }
}
