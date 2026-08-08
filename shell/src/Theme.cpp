// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#include "Theme.h"

#include <QFontDatabase>
#include <QFontMetricsF>
#include <QtMath>

Theme::Theme()
{
    for (uint32_t i = 0; i < 256; i++) {
        uint8_t r = 0, g = 0, b = 0;
        tt_palette_rgb(i, &r, &g, &b);
        m_palette[i] = QColor(r, g, b);
    }

    // `ttset.c`'s per-key defaults. The values look arbitrary and are not:
    // this is what Tera Term looks like out of the box, blue bold and all.
    m_normal[0] = QColor(0, 0, 0);            // VTColor, :754
    m_normal[1] = QColor(255, 255, 255);
    m_bold[0] = QColor(0, 0, 255);            // VTBoldColor, :757
    m_bold[1] = QColor(255, 255, 255);
    m_blink[0] = QColor(255, 0, 0);           // VTBlinkColor, :762
    m_blink[1] = QColor(255, 255, 255);
    m_underline[0] = QColor(255, 0, 255);     // VTUnderlineColor, :786
    m_underline[1] = QColor(255, 255, 255);
    m_reverse[0] = QColor(255, 255, 255);     // VTReverseColor, :767 — off by
    m_reverse[1] = QColor(0, 0, 0);           // default, so normally unused
    m_cursor = QColor(0, 0, 0);

    m_font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    m_font.setPointSizeF(11.0);
    setFont(m_font);
}

void Theme::setFont(const QFont &font)
{
    m_font = font;
    // Ask for the face without any spacing of ours, then add exactly enough to
    // land on the grid — see recomputeMetrics.
    m_font.setLetterSpacing(QFont::AbsoluteSpacing, 0.0);
    recomputeMetrics();
}

void Theme::recomputeMetrics()
{
    QFontMetricsF fm(m_font);

    // A run of text is drawn in one `drawText` call, which means the font's
    // own advance decides where each glyph inside the run lands. Even a
    // monospace face rarely advances by a whole number of device pixels, so a
    // run of 80 cells drifts off the grid and the cursor stops lining up with
    // the character under it.
    //
    // Rounding the advance to a cell and then *telling the font about it* with
    // absolute letter spacing fixes the drift at the source, so runs can be
    // batched freely rather than being drawn a cell at a time to keep them
    // honest.
    const qreal advance = fm.horizontalAdvance(QLatin1Char('M'));
    m_cellW = qMax(1, qRound(advance));
    m_font.setLetterSpacing(QFont::AbsoluteSpacing, m_cellW - advance);

    m_cellH = qMax(1, qCeil(fm.height()));
    m_baseline = qCeil(fm.ascent());

    m_boldFont = m_font;
    m_boldFont.setBold(true);
}

void Theme::resolve(const TtCell &cell, bool selected, bool screenReverse,
                    QColor *fg, QColor *bg) const
{
    const uint32_t attrs = cell.attrs;

    // Upstream composes the attribute with the setting that enables it before
    // looking at anything, so an attribute whose colour is switched off falls
    // through to the normal pair rather than to a disabled-looking one.
    const bool bold = m_boldColor && (attrs & TT_ATTR_BOLD);
    const bool blink = m_blinkColor && (attrs & TT_ATTR_BLINK);
    const bool underline = m_underlineColor && (attrs & TT_ATTR_UNDER);

    bool reverse = selected;
    if (attrs & TT_ATTR_REVERSE) {
        reverse = !reverse;
    }
    if (screenReverse) {
        reverse = !reverse;
    }

    // Blink beats bold beats underline. Only one pair applies; they do not mix.
    const QColor *pair = nullptr;
    if (blink) {
        pair = m_blink;
    } else if (bold) {
        pair = m_bold;
    } else if (underline) {
        pair = m_underline;
    }
    // `AttrURL` has no arm here because the VT engine never sets it: detecting
    // a URL in the buffer is a Tera Term *display* feature, and it is not
    // ported.

    if (!pair) {
        if (!reverse) {
            *fg = m_normal[0];
            *bg = m_normal[1];
        } else if (!m_reverseColor) {
            *fg = m_normal[1];
            *bg = m_normal[0];
        } else {
            *fg = m_reverse[0];
            *bg = m_reverse[1];
        }
    } else if (!reverse) {
        *fg = pair[0];
        *bg = m_useNormalBg ? m_normal[1] : pair[1];
    } else {
        *fg = m_useNormalBg ? m_normal[1] : pair[1];
        *bg = pair[0];
    }

    // And an explicit SGR colour wins over all of it.
    //
    // The bit is what makes this correct: `fg`/`bg` are a palette index *only*
    // when `TT_ATTR2_FORE` / `TT_ATTR2_BACK` says so. Without it the cell is
    // asking for the configured default, and painting index 0 anyway gives a
    // black-on-black screen that looks like a parser bug.
    //
    // The index is used as-is. Upstream runs it through `Get16ColorIndex`
    // first, which is the identity for everything except its 8-colour and
    // PC-style-16 modes — and both of those are off in a stock Tera Term. When
    // the settings schema makes `ColorFlag` reachable, that function is what
    // goes here.
    if (m_ansiColor && (attrs & TT_ATTR2_FORE)) {
        const QColor &c = paletteColor(cell.fg);
        if (!reverse) {
            *fg = c;
        } else {
            *bg = c;
        }
    }
    if (m_ansiColor && (attrs & TT_ATTR2_BACK)) {
        const QColor &c = paletteColor(cell.bg);
        if (!reverse) {
            *bg = c;
        } else {
            *fg = c;
        }
    }
}
