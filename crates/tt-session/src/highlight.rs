//! Highlight rules — the engine, and what it does to a line of cells.
//!
//! `tt-config`'s [`highlight`](tt_config::highlight) module owns the file; this
//! owns what a pattern *means*. The split is the same one `tt-macro` draws
//! around Oniguruma: reading a settings file should not oblige anybody to
//! compile a regex engine.
//!
//! **Matching happens at paint time, over the rows that are visible.** Nothing
//! is stamped into a cell as it arrives, which is what makes a new rule apply
//! to text already on the screen — and to the scrollback, which is the half of
//! that people notice. It is also why the engine below is the `regex` crate and
//! not the Oniguruma next door: this runs on the UI thread inside `paintEvent`
//! and the far end chooses the haystack, so linear time is a safety property
//! rather than a performance one.
//!
//! A rule matches the **logical** line — a wrapped command is one line to the
//! person reading it, so it is one line to a pattern, and a match that straddles
//! the wrap is coloured on both rows.

use regex::{Regex, RegexSet};
use tt_config::highlight::{Rule, Scope, STYLE_BOLD, STYLE_REVERSE, STYLE_UNDERLINE};
use tt_grid::{ATTR_BOLD, ATTR_REVERSE, ATTR_UNDER};

/// A colour a rule asked for.
pub type Rgb = [u8; 3];

/// One run of columns on one row, and what to do to it.
///
/// `from`..`to` are columns, `to` exclusive, and a wide character's padding
/// column is inside the run — so the span says "these columns", the same shape
/// as a selection range, and the painter needs no special case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub from: u16,
    pub to: u16,
    /// `None` leaves the cell's own foreground alone, which is how a rule
    /// changes only the background.
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    /// `tt_grid` attribute bits to OR into the cell for drawing. Not written
    /// into the grid — highlighting never changes what the terminal *is*.
    pub attrs: u32,
}

/// One run of plain text a rule claimed — [`Matcher::preview`]'s answer.
///
/// `from`..`to` are byte offsets into the text that was matched, always on
/// character boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSpan {
    pub from: u32,
    pub to: u32,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub attrs: u32,
}

/// A rule that would not compile, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rejected {
    /// Its position in the list it came from, counting from 1 the way the INI
    /// keys do, so a message can name `Highlight3Pattern`.
    pub index: usize,
    pub label: String,
    pub reason: String,
}

/// The pattern a rule hands the engine.
///
/// One place decides this, because three callers have to agree about it: the
/// matcher, the editor's live check, and any message naming a rule that failed.
pub fn effective_pattern(rule: &Rule) -> String {
    let body = if rule.literal {
        regex::escape(&rule.pattern)
    } else {
        rule.pattern.clone()
    };
    if rule.ignore_case {
        format!("(?i){body}")
    } else {
        body
    }
}

/// Compile one rule's pattern, for the editor to validate as it is typed.
pub fn check(rule: &Rule) -> Result<(), String> {
    Regex::new(&effective_pattern(rule))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// `tt-config`'s style words as `tt-grid` attribute bits.
///
/// The two numberings are deliberately separate — the config crate describes a
/// file and knows nothing about cells — so this is where they meet.
fn attrs_of(style: u32) -> u32 {
    let mut out = 0;
    if style & STYLE_BOLD != 0 {
        out |= ATTR_BOLD;
    }
    if style & STYLE_UNDERLINE != 0 {
        out |= ATTR_UNDER;
    }
    if style & STYLE_REVERSE != 0 {
        out |= ATTR_REVERSE;
    }
    out
}

/// What one cell has been claimed for, while a line is being worked out.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Style {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    attrs: u32,
}

impl Style {
    fn is_empty(&self) -> bool {
        self.fg.is_none() && self.bg.is_none() && self.attrs == 0
    }
}

struct Compiled {
    re: Regex,
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    attrs: u32,
    scope: Scope,
    group: u32,
}

/// The compiled rule set.
#[derive(Default)]
pub struct Matcher {
    rules: Vec<Compiled>,
    /// All the patterns at once, as a prefilter: one pass over the line says
    /// which rules are worth running, instead of one pass per rule. `None` if
    /// the set itself would not build, in which case every rule is run — a
    /// slower answer, never a different one.
    set: Option<RegexSet>,
    rejected: Vec<Rejected>,
}

impl Matcher {
    /// Compile what can be compiled.
    ///
    /// A rule whose pattern the engine refuses is dropped and recorded rather
    /// than failing the whole set: the others are somebody's working
    /// configuration and a hand-edited typo must not take them down with it.
    /// Disabled rules, empty patterns and rules that would paint nothing are
    /// left out too — there is no point running a pattern whose answer cannot
    /// change a pixel.
    pub fn new(rules: &[Rule]) -> Matcher {
        let mut compiled = Vec::new();
        let mut patterns = Vec::new();
        let mut rejected = Vec::new();
        for (n, rule) in rules.iter().enumerate() {
            if !rule.enabled || rule.pattern.is_empty() || !rule.paints() {
                continue;
            }
            let pattern = effective_pattern(rule);
            match Regex::new(&pattern) {
                Ok(re) => {
                    compiled.push(Compiled {
                        re,
                        fg: rule.fore,
                        bg: rule.back,
                        attrs: attrs_of(rule.style),
                        scope: rule.scope,
                        group: rule.group,
                    });
                    patterns.push(pattern);
                }
                Err(e) => rejected.push(Rejected {
                    index: n + 1,
                    label: rule.label.clone(),
                    reason: e.to_string(),
                }),
            }
        }
        let set = RegexSet::new(&patterns).ok();
        Matcher {
            rules: compiled,
            set,
            rejected,
        }
    }

    /// Whether asking this anything is worth the call.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The rules that would not compile, for a frontend to complain about once.
    pub fn rejected(&self) -> &[Rejected] {
        &self.rejected
    }

    /// Spans over one line of plain text, for the editor's preview.
    ///
    /// The editor shows the user's own sample line coloured by the rules they
    /// are writing, and it has to be coloured by *this* engine or the preview
    /// would be a second implementation quietly disagreeing with the first.
    ///
    /// Offsets are **bytes** into `text` rather than columns, because a preview
    /// is drawn as text and not on a grid.
    pub fn preview(&self, text: &str) -> Vec<TextSpan> {
        let mut starts: Vec<u32> = text.char_indices().map(|(i, _)| i as u32).collect();
        let cells = starts.len();
        starts.push(text.len() as u32);
        let mut styles = vec![Style::default(); cells];
        self.paint(text, &starts, &mut styles);

        let mut out: Vec<TextSpan> = Vec::new();
        for (i, style) in styles.iter().enumerate() {
            if style.is_empty() {
                continue;
            }
            let (from, to) = (starts[i], starts[i + 1]);
            match out.last_mut() {
                Some(last)
                    if last.to == from
                        && last.fg == style.fg
                        && last.bg == style.bg
                        && last.attrs == style.attrs =>
                {
                    last.to = to;
                }
                _ => out.push(TextSpan {
                    from,
                    to,
                    fg: style.fg,
                    bg: style.bg,
                    attrs: style.attrs,
                }),
            }
        }
        out
    }

    /// Claim cells of one logical line.
    ///
    /// `starts` holds the byte offset of each cell's text within `text`, plus a
    /// final sentinel, so it is one longer than `styles`.
    ///
    /// Rules are applied in list order and each fills only the channels no
    /// earlier rule has claimed, which is what lets a rule that only underlines
    /// compose with one that only colours instead of one silently swallowing
    /// the other. Attributes simply accumulate: two rules both asking for bold
    /// are not in disagreement.
    pub(crate) fn paint(&self, text: &str, starts: &[u32], styles: &mut [Style]) {
        debug_assert_eq!(starts.len(), styles.len() + 1);
        if styles.is_empty() {
            return;
        }
        // `RegexSet::matches` yields ascending indices, so the prefilter does
        // not disturb the order the rules were written in — which is their
        // priority.
        let hits: Vec<usize> = match &self.set {
            Some(set) => set.matches(text).into_iter().collect(),
            None => (0..self.rules.len()).collect(),
        };
        for i in hits {
            let rule = &self.rules[i];
            let mut claim = |start: usize, end: usize| {
                // A zero-width match — `x*` against a line with no `x` — names
                // no cell, and painting from its position would colour one the
                // pattern never touched.
                if end <= start {
                    return;
                }
                let len = styles.len();
                let (from, to) = match rule.scope {
                    Scope::Line => (0, len),
                    Scope::Match => (
                        cell_of(starts, start),
                        (cell_of(starts, end - 1) + 1).min(len),
                    ),
                };
                for style in &mut styles[from..to] {
                    style.fg = style.fg.or(rule.fg);
                    style.bg = style.bg.or(rule.bg);
                    style.attrs |= rule.attrs;
                }
            };
            if rule.group == 0 {
                for m in rule.re.find_iter(text) {
                    claim(m.start(), m.end());
                }
            } else {
                for caps in rule.re.captures_iter(text) {
                    // A group that did not take part in the match colours
                    // nothing, which is what `(a)|(b)` should do.
                    if let Some(m) = caps.get(rule.group as usize) {
                        claim(m.start(), m.end());
                    }
                }
            }
        }
    }
}

/// Which cell a byte offset belongs to.
///
/// Shared with [`crate::find`], which asks the same question of the same
/// [`Flattened`] — one byte-to-cell map in the tree, so a match cannot be
/// painted over one run of columns and stepped to at another.
pub(crate) fn cell_of(starts: &[u32], byte: usize) -> usize {
    starts
        .partition_point(|&o| o as usize <= byte)
        .saturating_sub(1)
        .min(starts.len().saturating_sub(2))
}

/// One logical line, flattened for matching.
///
/// The three vectors are parallel: `starts[i]` is where cell `i`'s text begins
/// in `text` (with a sentinel at the end), and `cells[i]` is where that cell is
/// on screen. Padding cells are not entries of their own — a wide character is
/// one cell two columns wide, which is what `width` carries.
#[derive(Default)]
pub(crate) struct Flattened {
    pub text: String,
    pub starts: Vec<u32>,
    /// `(absolute line, column, columns wide)`.
    pub cells: Vec<(u64, u16, u16)>,
}

impl Flattened {
    pub fn clear(&mut self) {
        self.text.clear();
        self.starts.clear();
        self.cells.clear();
    }

    /// Run-length encode `styles` into one span list per row.
    ///
    /// `out`'s vectors are reused rather than rebuilt: this runs once per
    /// logical line per frame, and a screenful of short lines would otherwise
    /// be a fresh allocation for each of them at 125 frames a second.
    ///
    /// A span breaks at a row boundary, at a change of style, and at a gap in
    /// the columns — the last of which cannot happen while cells are walked in
    /// order, and is one comparison against the day something leaves one.
    pub fn spans_into(&self, styles: &[Style], out: &mut Vec<(u64, Vec<Span>)>) {
        let mut used = 0;
        let mut i = 0;
        while i < self.cells.len() {
            let line = self.cells[i].0;
            if used == out.len() {
                out.push((line, Vec::new()));
            }
            let slot = &mut out[used];
            slot.0 = line;
            slot.1.clear();
            let mut open: Option<Style> = None;
            while i < self.cells.len() && self.cells[i].0 == line {
                let (_, col, width) = self.cells[i];
                let style = styles[i];
                i += 1;
                if style.is_empty() {
                    // A break in the run, so the next claimed cell starts a
                    // new span even if it wants exactly the same thing.
                    open = None;
                    continue;
                }
                let extend = match (&open, slot.1.last()) {
                    (Some(previous), Some(span)) => *previous == style && span.to == col,
                    _ => false,
                };
                if extend {
                    if let Some(span) = slot.1.last_mut() {
                        span.to = col + width;
                    }
                } else {
                    slot.1.push(Span {
                        from: col,
                        to: col + width,
                        fg: style.fg,
                        bg: style.bg,
                        attrs: style.attrs,
                    });
                    open = Some(style);
                }
            }
            used += 1;
        }
        out.truncate(used);
    }

    #[cfg(test)]
    fn spans(&self, styles: &[Style]) -> Vec<(u64, Vec<Span>)> {
        let mut out = Vec::new();
        self.spans_into(styles, &mut out);
        out
    }
}

/// The last logical line worked out, so a wrapped line is scanned once per
/// frame rather than once for each row it occupies.
///
/// Correctness rests on `epoch`, which [`crate::Session::mark_damage`] moves:
/// anything that changes the grid also tells the frontend to repaint, so a memo
/// from before that point is never handed out.
#[derive(Default)]
pub(crate) struct Memo {
    pub epoch: u64,
    pub first: u64,
    pub last: u64,
    pub rows: Vec<(u64, Vec<Span>)>,
}

impl Memo {
    pub fn covers(&self, epoch: u64, line: u64) -> bool {
        self.epoch == epoch && line >= self.first && line <= self.last
    }

    pub fn row(&self, line: u64) -> &[Span] {
        self.rows
            .iter()
            .find(|(l, _)| *l == line)
            .map(|(_, spans)| spans.as_slice())
            .unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tt_config::highlight::STYLE_UNDERLINE;

    /// One row of narrow ASCII, which is what most of these need.
    fn flat(text: &str) -> (Flattened, Vec<Style>) {
        let mut f = Flattened::default();
        for (i, ch) in text.chars().enumerate() {
            f.starts.push(f.text.len() as u32);
            f.text.push(ch);
            f.cells.push((0, i as u16, 1));
        }
        f.starts.push(f.text.len() as u32);
        let styles = vec![Style::default(); f.cells.len()];
        (f, styles)
    }

    fn rule(pattern: &str) -> Rule {
        Rule {
            pattern: pattern.into(),
            fore: Some([255, 0, 0]),
            ..Rule::default()
        }
    }

    fn spans(rules: &[Rule], text: &str) -> Vec<Span> {
        let (f, mut styles) = flat(text);
        Matcher::new(rules).paint(&f.text, &f.starts, &mut styles);
        f.spans(&styles).into_iter().flat_map(|(_, s)| s).collect()
    }

    #[test]
    fn a_match_colours_its_own_columns_and_no_others() {
        let s = spans(&[rule("ERROR")], "an ERROR here");
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].from, s[0].to), (3, 8));
        assert_eq!(s[0].fg, Some([255, 0, 0]));
        assert_eq!(s[0].bg, None);
    }

    #[test]
    fn every_match_on_the_line_is_coloured() {
        let s = spans(&[rule("ab")], "ab cd ab");
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].from, s[0].to), (0, 2));
        assert_eq!((s[1].from, s[1].to), (6, 8));
    }

    #[test]
    fn a_line_scope_takes_the_whole_line() {
        let mut r = rule("ERROR");
        r.scope = Scope::Line;
        let s = spans(&[r], "an ERROR here");
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].from, s[0].to), (0, 13));
    }

    #[test]
    fn a_capture_group_colours_only_itself() {
        let mut r = rule("protocol is (\\w+)");
        r.group = 1;
        let s = spans(&[r], "line protocol is down");
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].from, s[0].to), (17, 21));
    }

    #[test]
    fn a_group_that_did_not_take_part_colours_nothing() {
        let mut r = rule("(up)|(down)");
        r.group = 1;
        assert!(spans(&[r.clone()], "it is down").is_empty());
        assert_eq!(spans(&[r], "it is up").len(), 1);
    }

    #[test]
    fn an_earlier_rule_keeps_the_channel_it_claimed_and_a_later_one_fills_the_rest() {
        // The composition rule: first wins per channel, so a rule that only
        // underlines and one that only colours do not swallow each other.
        let first = Rule {
            pattern: "ERROR".into(),
            fore: Some([1, 1, 1]),
            ..Rule::default()
        };
        let second = Rule {
            pattern: "ERROR".into(),
            fore: Some([2, 2, 2]),
            back: Some([3, 3, 3]),
            style: STYLE_UNDERLINE,
            ..Rule::default()
        };
        let s = spans(&[first, second], "an ERROR here");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].fg, Some([1, 1, 1]), "the first rule keeps it");
        assert_eq!(s[0].bg, Some([3, 3, 3]), "the second fills what was free");
        assert_eq!(s[0].attrs, ATTR_UNDER);
    }

    #[test]
    fn a_disabled_rule_and_one_that_paints_nothing_are_both_left_out() {
        let off = Rule {
            pattern: "a".into(),
            fore: Some([1, 1, 1]),
            enabled: false,
            ..Rule::default()
        };
        let inert = Rule {
            pattern: "a".into(),
            ..Rule::default()
        };
        let empty = Rule {
            fore: Some([1, 1, 1]),
            ..Rule::default()
        };
        let m = Matcher::new(&[off, inert, empty]);
        assert!(m.is_empty());
        assert!(m.rejected().is_empty());
    }

    #[test]
    fn a_pattern_the_engine_refuses_drops_that_rule_and_no_others() {
        let bad = Rule {
            pattern: "(unclosed".into(),
            label: "mine".into(),
            fore: Some([1, 1, 1]),
            ..Rule::default()
        };
        let good = rule("ok");
        let m = Matcher::new(&[bad, good]);
        assert_eq!(m.rejected().len(), 1);
        assert_eq!(m.rejected()[0].index, 1);
        assert_eq!(m.rejected()[0].label, "mine");
        assert!(!m.rejected()[0].reason.is_empty());
        assert!(!m.is_empty(), "the working rule survives its neighbour");
    }

    #[test]
    fn a_literal_pattern_is_not_a_pattern() {
        let mut r = rule("10.0.0.1");
        r.literal = true;
        assert_eq!(effective_pattern(&r), "10\\.0\\.0\\.1");
        assert!(spans(&[r.clone()], "10x0y0z1").is_empty());
        assert_eq!(spans(&[r], "at 10.0.0.1 now").len(), 1);
    }

    #[test]
    fn ignore_case_is_a_flag_and_not_a_second_pattern() {
        let mut r = rule("error");
        r.ignore_case = true;
        assert_eq!(effective_pattern(&r), "(?i)error");
        assert_eq!(spans(&[r], "an ERROR here").len(), 1);
    }

    #[test]
    fn a_zero_width_match_colours_nothing() {
        // `x*` matches at every position and consumes nothing at most of them.
        assert!(spans(&[rule("x*")], "abc").is_empty());
    }

    #[test]
    fn adjacent_cells_with_the_same_style_are_one_span() {
        let s = spans(&[rule("[ab]")], "abc");
        assert_eq!(s.len(), 1, "two matches, one run: {s:?}");
        assert_eq!((s[0].from, s[0].to), (0, 2));
    }

    #[test]
    fn a_wide_character_carries_its_padding_column() {
        let mut f = Flattened::default();
        // "あ!" — one double-width cell at column 0, then a narrow one at 2.
        for (i, (ch, col, width)) in [('あ', 0u16, 2u16), ('!', 2, 1)].iter().enumerate() {
            let _ = i;
            f.starts.push(f.text.len() as u32);
            f.text.push(*ch);
            f.cells.push((0, *col, *width));
        }
        f.starts.push(f.text.len() as u32);
        let mut styles = vec![Style::default(); f.cells.len()];
        Matcher::new(&[rule("あ")]).paint(&f.text, &f.starts, &mut styles);
        let spans = f.spans(&styles);
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].1[0].from, spans[0].1[0].to), (0, 2));
    }

    #[test]
    fn a_match_across_a_wrap_is_coloured_on_both_rows() {
        // Two rows of a logical line: "abcd" on line 7, "efgh" on line 8.
        let mut f = Flattened::default();
        for (line, text) in [(7u64, "abcd"), (8, "efgh")] {
            for (i, ch) in text.chars().enumerate() {
                f.starts.push(f.text.len() as u32);
                f.text.push(ch);
                f.cells.push((line, i as u16, 1));
            }
        }
        f.starts.push(f.text.len() as u32);
        let mut styles = vec![Style::default(); f.cells.len()];
        Matcher::new(&[rule("cdef")]).paint(&f.text, &f.starts, &mut styles);
        let spans = f.spans(&styles);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].0, 7);
        assert_eq!((spans[0].1[0].from, spans[0].1[0].to), (2, 4));
        assert_eq!(spans[1].0, 8);
        assert_eq!((spans[1].1[0].from, spans[1].1[0].to), (0, 2));
    }

    #[test]
    fn a_preview_answers_in_bytes_over_plain_text() {
        let m = Matcher::new(&[rule("ERROR")]);
        let spans = m.preview("an ERROR here");
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].from, spans[0].to), (3, 8));
        assert_eq!(spans[0].fg, Some([255, 0, 0]));
        // Bytes, not characters: the offsets have to index the UTF-8 the
        // editor is slicing.
        let spans = m.preview("héllo ERROR");
        assert_eq!((spans[0].from, spans[0].to), (7, 12));
        assert!(m.preview("nothing here").is_empty());
    }

    #[test]
    fn check_reports_what_the_engine_said() {
        assert!(check(&rule("fine")).is_ok());
        let bad = check(&rule("(unclosed"));
        assert!(bad.is_err());
        assert!(
            bad.unwrap_err().contains("unclosed"),
            "the engine's own words"
        );
    }
}
