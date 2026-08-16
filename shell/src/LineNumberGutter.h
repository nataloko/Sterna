// The column of line numbers down the left of a terminal.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QWidget>

class Session;
class Theme;

/// Line numbers, painted **beside** the terminal rather than into it.
///
/// That placement is the whole design, and it is what makes the numbers
/// impossible to copy. `TerminalView::selectedText` builds the clipboard string
/// out of cells the core owns, and the log, the printer and a macro's tap all
/// read the same grid — so a widget that is not the grid cannot contribute a
/// character to any of them. Nothing has to remember to strip anything.
///
/// The alternative, reserving columns inside the view, would have needed an
/// origin offset in the painter, in both hit-testing functions, in the five
/// places that hand raw pixels to the core's mouse reporting, in the sixel and
/// cursor placement, in the line editor and in `refit` — a dozen chances to get
/// the clipboard wrong by getting an offset wrong.
///
/// The view has since grown a horizontal origin anyway, for a window narrower
/// than its terminal, and it is worth saying how little that changes here. It
/// is one `QPainter::translate` and one `TerminalView::gridPos`, so it exists
/// in two places rather than a dozen — and it moves *terminal columns* under a
/// viewport, which is a different question from taking columns away from the
/// terminal. This widget still owns no cell, and that is still what keeps the
/// numbers out of every copy.
///
/// A plain `QWidget` and not a `QAbstractScrollArea` or a `QListView`, for the
/// same reason `TerminalView` is one: this draws digits in cell coordinates and
/// scrolls only because the rows underneath it do.
class LineNumberGutter : public QWidget {
    Q_OBJECT

public:
    LineNumberGutter(const Session *session, const Theme &theme,
                     QWidget *parent = nullptr);

    /// How many digits to reserve — `terminal.line_number_width`.
    ///
    /// Fixed rather than measured against the largest number on screen: a
    /// gutter that widened at line 1000 would re-flow the terminal underneath
    /// somebody's cursor. A number too long for its field is therefore not
    /// drawn at all — see `paintEvent`, where the alternative is a *wrong*
    /// number rather than an untidy one. Six digits by default, which is a
    /// million lines: the number a session can reach has no ceiling, so a
    /// default of four went blank a few minutes into any `cat`.
    void setDigits(int digits);
    int digits() const { return m_digits; }

    /// Start the count again: the next line the host prints is line 1.
    ///
    /// The mark is placed one line *below* the cursor because a counter is
    /// reset before the thing it is going to count — somebody at a prompt about
    /// to run a command wants that command's first line of output to be 1, and
    /// the line they are standing on is the prompt. It is held as an absolute
    /// line number, so the newline that follows may scroll the page as much as
    /// it likes without moving the mark.
    ///
    /// A line above the mark carries no number at all, rather than a zero or a
    /// negative one: it was printed before there was a counter to count it. So
    /// the gutter goes blank from the cursor upwards until the host prints
    /// something, which is why the window says what it has done in the status
    /// line — a column of numbers that vanishes with nothing to explain it is
    /// the sort of thing that reads as a bug.
    ///
    /// Display only, and deliberately not a setting: a mark is a moment in one
    /// session, and a saved one would number the next session from a point that
    /// never happened in it.
    void resetCounter();

    /// The absolute line the count starts from — zero, the session's own first
    /// line, until something resets it.
    quint64 origin() const { return m_origin; }

    /// Re-measure after the font or the cell size moved.
    void updateMetrics();

    /// Where a wheel notch over the gutter goes.
    ///
    /// The terminal, because a person reading line 400 and rolling the wheel
    /// is scrolling the *terminal* — they have not aimed at a widget, they have
    /// aimed at the text. The alternative is a five-column dead strip down the
    /// side of the window, which is the kind of thing that reads as a bug.
    /// Clicks are not forwarded: the gutter has nothing to select, and a drag
    /// that started here would have no honest first character.
    void setWheelTarget(QWidget *target) { m_wheelTarget = target; }

    QSize sizeHint() const override;

protected:
    void paintEvent(QPaintEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;

private:
    int widthForDigits() const;

    const Session *m_session;
    const Theme &m_theme;
    QWidget *m_wheelTarget = nullptr;
    int m_digits = 6;
    quint64 m_origin = 0;
};
