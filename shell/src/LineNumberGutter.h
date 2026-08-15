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
    /// somebody's cursor. A number too long for its field overflows to the left
    /// instead, which is ugly for one session rather than disruptive for every
    /// session.
    void setDigits(int digits);
    int digits() const { return m_digits; }

    /// Re-measure after the font or the cell size moved.
    void updateMetrics();

    QSize sizeHint() const override;

protected:
    void paintEvent(QPaintEvent *event) override;

private:
    int widthForDigits() const;

    const Session *m_session;
    const Theme &m_theme;
    int m_digits = 4;
};
