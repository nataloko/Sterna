// Colours and font metrics for the grid painter.
//
// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QColor>
#include <QFont>

#include "termitta.h"

/// How a cell's attributes become two colours, and how big a character cell
/// is.
///
/// The colour half is a port of `vtdisp.c:GetDrawAttr`, not an invention. It
/// looks more elaborate than "foreground and background" because Tera Term's
/// bold, blink and underline attributes each carry their *own* colour pair,
/// and which pair wins is a priority chain rather than a blend. Guessing here
/// would give a terminal that renders a real host's output visibly differently
/// from the thing this project is a successor to.
///
/// The defaults are upstream's `TERATERM.INI` defaults, which is why the
/// terminal is black on white and bold text comes out blue. Every one of them
/// is an INI key, so they belong to Stage 2's generated settings schema; they
/// are constants here rather than a config file so that the schema is the only
/// thing that ever parses them.
class Theme {
public:
    Theme();

    /// Resolve one cell to the two colours it is painted with.
    ///
    /// `selected` is the frontend's own highlight and `screenReverse` is
    /// DECSCNM. Both fold into the same reverse flag the cell's own
    /// `TT_ATTR_REVERSE` does, and three reverses is still a reverse — which
    /// is why they are counted rather than checked in turn.
    void resolve(const TtCell &cell, bool selected, bool screenReverse,
                 QColor *fg, QColor *bg) const;

    /// The palette entry, straight from the core. Painting through this rather
    /// than through a table of our own is deliberate: the grid stores an
    /// *index* because Tera Term does, and the index was chosen by upstream's
    /// nearest-colour search against upstream's palette.
    const QColor &paletteColor(uint32_t index) const { return m_palette[index & 0xFF]; }

    const QColor &defaultBackground() const { return m_normal[1]; }
    const QColor &cursorColor() const { return m_cursor; }

    const QFont &font() const { return m_font; }
    /// The same face, bolded — cached because constructing a QFont per run of
    /// bold text shows up in a profile immediately.
    const QFont &boldFont() const { return m_boldFont; }
    void setFont(const QFont &font);

    int cellWidth() const { return m_cellW; }
    int cellHeight() const { return m_cellH; }
    /// Distance from the top of the cell to the text baseline.
    int baseline() const { return m_baseline; }

private:
    void recomputeMetrics();

    QColor m_palette[256];
    // Each pair is { foreground, background }, as upstream stores them.
    QColor m_normal[2];
    QColor m_bold[2];
    QColor m_blink[2];
    QColor m_underline[2];
    QColor m_reverse[2];
    QColor m_cursor;

    // `ts.ColorFlag` bits, and `ts.UseNormalBGColor`. Fixed at upstream's
    // defaults until the settings schema exists.
    bool m_ansiColor = true;
    bool m_boldColor = true;
    bool m_blinkColor = true;
    bool m_underlineColor = true;
    bool m_reverseColor = false;
    bool m_useNormalBg = false;

    QFont m_font;
    QFont m_boldFont;
    int m_cellW = 8;
    int m_cellH = 16;
    int m_baseline = 12;
};
