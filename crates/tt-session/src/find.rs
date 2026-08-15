//! Find — what a search pattern means, and where its answers live.
//!
//! Sibling of [`highlight`](crate::highlight), and deliberately built on it:
//! the hard part of both is the same part, which is that a *line* on a terminal
//! is not a row. A command long enough to wrap is one line to whoever typed it,
//! so a match straddling the wrap has to be findable, and a `$` has to mean the
//! end of the text rather than the right margin. That work is
//! [`crate::Session::flatten_into`]'s, and find borrows it whole rather than
//! growing a second answer to it.
//!
//! Same [`regex`] engine as highlight rules, for the same reason — this runs on
//! the UI thread over a haystack the far end chose, so linear time is a safety
//! property — and therefore the same syntax, which `docs/find.md` sends the
//! reader to `docs/highlighting.md` for rather than describing twice.
//!
//! **Nothing here is cached across output.** A hit is a position in a buffer
//! the host is still writing to, so retaining a list of them would mean
//! retaining a rule for when the list stops being true. Instead
//! [`crate::Session::find_next`] scans live and stops at the first hit, and the
//! painting path matches the visible row on demand exactly as
//! [`crate::Session::row_highlights`] does. There is no staleness to revalidate
//! because there is nothing kept.

use regex::Regex;

use crate::highlight::{cell_of, Flattened};

/// What to look for.
///
/// `literal` and `whole_word` are spellings of a *pattern*, not a second
/// matcher — see [`effective_pattern`]. One engine, three checkboxes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub pattern: String,
    /// The pattern is text to be found, not an expression.
    pub literal: bool,
    pub ignore_case: bool,
    /// Only a match with a word boundary at each end counts.
    pub whole_word: bool,
}

/// Where one match is.
///
/// Absolute line numbers and column boundaries — the coordinates a selection is
/// held in (`TerminalView`'s `SelPoint`), because they are the ones that
/// survive the host printing underneath them. Two line numbers because a match
/// can straddle a soft wrap: `line`/`from` is where it starts, `end_line`/`to`
/// where it ends, `to` exclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hit {
    pub line: u64,
    pub from: u16,
    pub end_line: u64,
    pub to: u16,
}

impl Hit {
    /// Where a forward search resuming from this hit should start: past its
    /// end, not past its beginning.
    ///
    /// That is [`Regex::find_iter`]'s own rule, and stepping has to keep it or
    /// the count and the steps disagree — `aa` in `aaaa` would be two matches
    /// in the label and three to somebody pressing Next, which reads as the
    /// count being wrong.
    pub fn after(&self) -> (u64, u16) {
        (self.end_line, self.to)
    }

    /// The mirror, for a backward search: everything strictly before the start.
    pub fn before(&self) -> (u64, u16) {
        (self.line, self.from)
    }
}

/// One run of columns on one row, for painting.
///
/// The same shape as [`crate::highlight::Span`] with the colours left out —
/// what a match should *look* like is the frontend's `color.find`, not the
/// engine's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub from: u16,
    pub to: u16,
}

/// The last logical line matched, so a wrapped line is scanned once per frame
/// rather than once for each row it occupies.
///
/// Same contract as [`crate::highlight::Memo`] and for the same reason: `epoch`
/// is [`crate::Session::mark_damage`]'s, so a memo from before the grid changed
/// is never handed out.
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

/// One match, as cell indices into a [`Flattened`].
#[derive(Clone, Copy)]
struct Cells {
    first: usize,
    last: usize,
}

/// Where a match sits, in cells.
///
/// A zero-width match — `x*` against a line with no `x` — names no cell, so it
/// is not a place a Next button could take anybody and is dropped here rather
/// than at each of the three callers.
fn cells_of(flat: &Flattened, m: regex::Match<'_>) -> Option<Cells> {
    if m.end() <= m.start() || flat.cells.is_empty() {
        return None;
    }
    let first = cell_of(&flat.starts, m.start());
    let last = cell_of(&flat.starts, m.end() - 1).min(flat.cells.len() - 1);
    Some(Cells { first, last })
}

/// Turn a cell range into the coordinates a selection is held in.
fn hit_of(flat: &Flattened, cells: Cells) -> Hit {
    let (line, from, _) = flat.cells[cells.first];
    let (end_line, col, width) = flat.cells[cells.last];
    Hit {
        line,
        from,
        end_line,
        to: col + width,
    }
}

/// The first cell at or after `(line, x)`, or the cell count when there is
/// none.
///
/// Cells are walked in order, so this is where a forward search inside one
/// logical line starts. `x` past the end of a row simply lands on the next
/// row's first cell, which is what makes stepping off the right margin work
/// without the caller knowing how wide the terminal is.
fn cell_at(flat: &Flattened, line: u64, x: u16) -> usize {
    flat.cells
        .iter()
        .position(|&(l, col, _)| (l, col) >= (line, x))
        .unwrap_or(flat.cells.len())
}

/// The first match of `re` in `flat` starting at or after `(line, x)`.
///
/// `None` for "not in this logical line", which is what makes the walk above
/// this one a simple loop.
pub(crate) fn first_at_or_after(re: &Regex, flat: &Flattened, line: u64, x: u16) -> Option<Hit> {
    let start = cell_at(flat, line, x);
    re.find_iter(&flat.text)
        .filter_map(|m| cells_of(flat, m))
        .find(|c| c.first >= start)
        .map(|c| hit_of(flat, c))
}

/// The last match of `re` in `flat` starting strictly before `(line, x)`.
pub(crate) fn last_before(re: &Regex, flat: &Flattened, line: u64, x: u16) -> Option<Hit> {
    let start = cell_at(flat, line, x);
    re.find_iter(&flat.text)
        .filter_map(|m| cells_of(flat, m))
        .take_while(|c| c.first < start)
        .last()
        .map(|c| hit_of(flat, c))
}

/// Every match in `flat`, as painted column runs, appended per row to `out`.
///
/// A claim map rather than spans built as the matches arrive: two matches can
/// land on the same row with a gap between them and one match can cross rows,
/// and marking cells first makes both fall out of one run-length pass.
pub(crate) fn runs_into(
    re: &Regex,
    flat: &Flattened,
    claimed: &mut Vec<bool>,
    out: &mut Vec<(u64, Vec<Span>)>,
) {
    claimed.clear();
    claimed.resize(flat.cells.len(), false);
    for m in re.find_iter(&flat.text) {
        if let Some(c) = cells_of(flat, m) {
            for cell in &mut claimed[c.first..=c.last] {
                *cell = true;
            }
        }
    }

    let mut used = 0;
    let mut i = 0;
    while i < flat.cells.len() {
        let line = flat.cells[i].0;
        if used == out.len() {
            out.push((line, Vec::new()));
        }
        let slot = &mut out[used];
        slot.0 = line;
        slot.1.clear();
        let mut open = false;
        while i < flat.cells.len() && flat.cells[i].0 == line {
            let (_, col, width) = flat.cells[i];
            let mark = claimed[i];
            i += 1;
            if !mark {
                open = false;
                continue;
            }
            match slot.1.last_mut() {
                Some(span) if open && span.to == col => span.to = col + width,
                _ => {
                    slot.1.push(Span {
                        from: col,
                        to: col + width,
                    });
                    open = true;
                }
            }
        }
        used += 1;
    }
    out.truncate(used);
}

/// How many matches `flat` holds.
pub(crate) fn count_in(re: &Regex, flat: &Flattened) -> usize {
    re.find_iter(&flat.text)
        .filter(|m| m.end() > m.start())
        .count()
}

/// The pattern the engine is actually given.
///
/// Composed rather than branched, so that the three checkboxes cannot disagree
/// about anything: `literal` decides whether the text is escaped, `whole_word`
/// wraps whatever came out of that in boundaries, and `ignore_case` prefixes
/// the lot. The non-capturing group around the body is what makes whole word
/// work on an alternation — `\bfoo|bar\b` is not what the box means.
pub fn effective_pattern(query: &Query) -> String {
    let mut body = if query.literal {
        regex::escape(&query.pattern)
    } else {
        query.pattern.clone()
    };
    if query.whole_word && !body.is_empty() {
        body = format!(r"\b(?:{body})\b");
    }
    if query.ignore_case {
        format!("(?i){body}")
    } else {
        body
    }
}

/// Compile a query, for a find bar to complain as it is typed.
pub fn check(query: &Query) -> Result<(), String> {
    compile(query).map(|_| ()).map_err(|e| e.to_string())
}

/// The compiled query, or an error the user can be shown.
///
/// An empty pattern compiles to a regex that matches everywhere, which would
/// paint the whole screen and step through every character; it is not an error
/// either, because an empty find field is somebody who has not typed yet. So it
/// is `None` — nothing to look for — and every caller treats that as no matches
/// rather than as a failure.
pub(crate) fn compile(query: &Query) -> Result<Option<Regex>, regex::Error> {
    if query.pattern.is_empty() {
        return Ok(None);
    }
    Regex::new(&effective_pattern(query)).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_query_escapes_its_metacharacters() {
        let q = Query {
            pattern: "a.c".into(),
            literal: true,
            ..Query::default()
        };
        let re = compile(&q).unwrap().unwrap();
        assert!(re.is_match("a.c"));
        assert!(!re.is_match("abc"));
    }

    #[test]
    fn whole_word_wraps_the_whole_alternation_and_not_its_first_branch() {
        let q = Query {
            pattern: "foo|bar".into(),
            whole_word: true,
            ..Query::default()
        };
        assert_eq!(effective_pattern(&q), r"\b(?:foo|bar)\b");
        let re = compile(&q).unwrap().unwrap();
        assert!(re.is_match("a bar here"));
        assert!(!re.is_match("crowbars"));
    }

    #[test]
    fn ignore_case_applies_to_the_composed_pattern() {
        let q = Query {
            pattern: "err".into(),
            literal: true,
            ignore_case: true,
            whole_word: true,
        };
        assert_eq!(effective_pattern(&q), r"(?i)\b(?:err)\b");
        assert!(compile(&q).unwrap().unwrap().is_match("an ERR happened"));
    }

    /// An empty field is not a pattern that matches everywhere, and not an
    /// error either.
    #[test]
    fn an_empty_pattern_is_nothing_to_look_for() {
        assert!(compile(&Query::default()).unwrap().is_none());
        assert!(check(&Query::default()).is_ok());
    }

    #[test]
    fn a_pattern_the_engine_refuses_says_why() {
        let q = Query {
            pattern: "(unclosed".into(),
            ..Query::default()
        };
        assert!(check(&q).is_err());
    }

    /// The same subset `docs/highlighting.md` documents, and the half of it
    /// that is a property of the engine rather than of a feature flag:
    /// backreferences and lookaround are what `regex` gives up to promise
    /// linear time, so a pattern using either is refused in every build.
    ///
    /// The other half — script and age classes, which the workspace pins
    /// `regex` without — is deliberately **not** asserted here. Cargo unifies
    /// features across everything one invocation builds, and `tt-fuzz`'s
    /// `proptest` asks for `regex-syntax`'s defaults, so `\p{Greek}` compiles
    /// under `cargo test` at the workspace root and not under
    /// `cargo test -p tt-session`. A test that reads the dependency graph
    /// rather than this crate is a test that fails depending on how it is run.
    #[test]
    fn the_engine_refuses_what_it_cannot_do_in_linear_time() {
        for pattern in [r"(a)\1", r"(?=foo)bar", r"(?<=foo)bar"] {
            let q = Query {
                pattern: pattern.into(),
                ..Query::default()
            };
            assert!(check(&q).is_err(), "{pattern} compiled");
        }
    }
}
