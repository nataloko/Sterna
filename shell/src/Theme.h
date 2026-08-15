// Colours and font metrics for the grid painter.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QColor>
#include <QFont>
#include <QString>

#include "sterna.h"

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
/// terminal is black on white and bold text comes out blue. They are compiled
/// in as a starting point and then **replaced from the settings** — see
/// [`applySettings`], which is the only thing here that reads a file, and does
/// it by asking the core rather than by parsing anything.
/// What a highlight rule asked for on one cell, from `Session::rowHighlights`.
///
/// Deliberately not part of the cell: a rule changes what is drawn and nothing
/// about what the terminal *is*, so the grid still says what the host sent and
/// the log, the clipboard and the printer never see any of this.
struct CellOverride {
    /// Invalid means "leave that one alone", which is how a rule changes only
    /// the background.
    QColor fg;
    QColor bg;
    /// `TT_ATTR_*` bits to OR in for drawing.
    quint32 attrs = 0;
};

class Theme {
public:
    Theme();

    /// Take the colour settings from a session's `TERATERM.INI`.
    ///
    /// Scalar colours and switches are addressed by name through the C ABI,
    /// so this holds no list of settings and no file parser. The palette is
    /// read as the session's already-parsed table because it also participates
    /// in truecolor resolution on the core side. A name the schema does not
    /// have leaves the compiled-in default standing.
    void applySettings(const class Session &session);

    /// Re-read only the colours — the palette and the six attribute pairs.
    ///
    /// The half of `applySettings` a host can change on its own, so it is also
    /// the whole answer to `Session::colorsChanged`. Everything else there is
    /// a setting and cannot move without the settings moving.
    void readColors(const class Session &session);

    /// Resolve one cell to the two colours it is painted with.
    ///
    /// `selected` is the frontend's own highlight and `screenReverse` is
    /// DECSCNM. Both fold into the same reverse flag the cell's own
    /// `TT_ATTR_REVERSE` does, and three reverses is still a reverse — which
    /// is why they are counted rather than checked in turn.
    ///
    /// `over` is a highlight rule's claim on this cell, or null. It is applied
    /// **after** everything upstream does, so nothing below can take it back —
    /// and it goes through the same reverse flag as an SGR colour, so dragging
    /// a selection across highlighted text still inverts it.
    void resolve(const TtCell &cell, bool selected, bool screenReverse,
                 QColor *fg, QColor *bg, const CellOverride *over = nullptr) const;

    /// The palette entry, straight from the core. Painting through this rather
    /// than through a table of our own is deliberate: the grid stores an
    /// *index* because Tera Term does, and the index was chosen by upstream's
    /// nearest-colour search against upstream's palette.
    const QColor &paletteColor(uint32_t index) const { return m_palette[index & 0xFF]; }

    /// The background a cell the host said nothing about is painted with.
    ///
    /// Not `m_normal[1]` itself: while nothing is connected this is that
    /// colour moved `color.disconnected_shade` percent of the way towards the
    /// foreground, so an idle terminal is a different shade at a glance. See
    /// `setConnected`.
    const QColor &defaultBackground() const { return m_background; }
    const QColor &defaultForeground() const { return m_normal[0]; }
    const QColor &cursorColor() const { return m_cursor; }

    /// What the line-number gutter writes its digits in.
    ///
    /// The foreground run part of the way toward the background — the same mix
    /// [`shaded`] does, in the other direction, and for the same reason: those
    /// two are the only colours a person choosing a terminal theme has
    /// guaranteed are visible against each other, so a fixed grey would
    /// disappear on half of them. Computed rather than stored because both ends
    /// move: a host's `OSC 10`/`11` and a switch to the dark palette each
    /// change it, and nothing would refresh a cached copy.
    ///
    /// Numbers are chrome, not content, so they should read as quieter than the
    /// text beside them without becoming unreadable — hence a mix rather than
    /// the foreground itself.
    QColor lineNumberColor() const;

    /// Whether this terminal has something on the other end.
    ///
    /// The one piece of session state the painter holds, and it is here rather
    /// than in the view because it moves a *colour*: `resolve` shades every
    /// background the host did not choose, so the shade covers the attribute
    /// pairs' backgrounds too and a screen with bold text on it does not come
    /// out in unshaded patches.
    ///
    /// Towards the foreground, which is why a reversed cell does not move: the
    /// blend's far end is the colour it is already painted in.
    void setConnected(bool connected);

    const QFont &font() const { return m_font; }
    /// The same face, bolded — cached because constructing a QFont per run of
    /// bold text shows up in a profile immediately.
    const QFont &boldFont() const { return m_boldFont; }
    bool paintsBold(uint32_t attrs) const
    {
        return m_boldFontEnabled && (attrs & TT_ATTR_BOLD) != 0;
    }
    bool paintsUnderline(uint32_t attrs) const
    {
        return (m_urlUnderlineEnabled && (attrs & TT_ATTR_URL) != 0) ||
               (m_underlineFontEnabled && (attrs & TT_ATTR_UNDER) != 0);
    }
    void setFont(const QFont &font);

    /// `DrawingResizedFont`: whether a glyph whose natural advance misses its
    /// cell box is stretched horizontally into that box.
    bool drawsResizedFont() const { return m_drawResizedFont; }
    bool shouldResizeGlyph(const QString &text, bool bold, int cells) const;

    int cellWidth() const { return m_cellW; }
    int cellHeight() const { return m_cellH; }
    /// The unpadded width `DrawingResizedFont` fits one cell's glyph into.
    int fontWidth() const { return m_fontW; }
    /// `VTFontSpace`'s left inset. Ordinary text uses it once; upstream's
    /// resized wide-glyph path multiplies it by the glyph's cell count.
    int textOffsetX() const { return m_spaceLeft; }
    /// Distance from the top of the cell to the text baseline.
    int baseline() const { return m_baseline; }

private:
    void applyDarkPalette();
    void recomputeMetrics();
    /// `m_normal[1]` moved towards `m_normal[0]`, or itself when connected.
    void updateBackground();
    QColor shaded(const QColor &background) const;

    QColor m_palette[256];
    // Each pair is { foreground, background }, as upstream stores them.
    QColor m_normal[2];
    QColor m_bold[2];
    QColor m_blink[2];
    QColor m_underline[2];
    QColor m_url[2];
    QColor m_reverse[2];
    QColor m_cursor;
    QColor m_background;

    // `color.disconnected_shade`, and whether it currently applies. A window
    // opens before it has connected, so the shade is on from the first frame.
    int m_shade = 12;
    bool m_connected = false;

    // `ts.ColorFlag`, `ts.FontFlag` and `ts.UseNormalBGColor`. Upstream's
    // defaults until `applySettings` reads the file.
    bool m_ansiColor = true;
    bool m_boldColor = true;
    bool m_blinkColor = true;
    bool m_underlineColor = true;
    bool m_urlColor = true;
    bool m_reverseColor = false;
    bool m_useTextColor = false;
    bool m_useNormalBg = false;
    bool m_boldFontEnabled = true;
    bool m_underlineFontEnabled = true;
    bool m_urlUnderlineEnabled = true;
    bool m_drawResizedFont = true;

    QFont m_font;
    QFont m_boldFont;
    int m_fontW = 8;
    int m_fontH = 16;
    int m_spaceLeft = 0;
    int m_spaceRight = 0;
    int m_spaceTop = 0;
    int m_spaceBottom = 0;
    int m_cellW = 8;
    int m_cellH = 16;
    int m_baseline = 12;
};
