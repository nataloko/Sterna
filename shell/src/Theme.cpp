// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Theme.h"

#include <QFontDatabase>
#include <QFontMetricsF>
#include <QtMath>

#include "Session.h"

namespace {

/// The schema normalises a boolean to `on` or `off` on the way out, whatever
/// the file said — so this is a comparison rather than a second parse of
/// `GetOnOff`, which is default-biased and belongs in exactly one place.
bool readFlag(const Session &session, const char *name, bool fallback)
{
    const QString value = session.setting(QString::fromLatin1(name));
    if (value.isEmpty()) {
        return fallback;
    }
    return value == QLatin1String("on");
}

int readInt(const Session &session, const char *name, int fallback)
{
    bool ok = false;
    const int value = session.setting(QString::fromLatin1(name)).toInt(&ok);
    return ok ? value : fallback;
}

} // namespace

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
    m_url[0] = QColor(0, 255, 0);             // URLColor, :775
    m_url[1] = QColor(255, 255, 255);
    m_reverse[0] = QColor(255, 255, 255);     // VTReverseColor, :767 — off by
    m_reverse[1] = QColor(0, 0, 0);           // default, so normally unused
    m_cursor = QColor(0, 0, 0);

    m_font = QFontDatabase::systemFont(QFontDatabase::FixedFont);
    m_font.setPointSizeF(11.0);
    setFont(m_font);
}

void Theme::readColors(const Session &session)
{
    // The parser resolves truecolor against this same live table, so the
    // painter must read it from the session too. Reading `ANSIColor` again in
    // Qt would create two parsers for its masking, wrapping and buffer limits.
    //
    // And the pairs come from the same place for a second reason on top of
    // that one: `OSC 4`/`5`/`10`-`19` move them while the session runs, so a
    // painter that read `color.normal` out of the settings would be showing
    // the file's answer to a question the host has since changed. The settings
    // are still what a `OSC 110` reset returns to — the core keeps both.
    for (uint32_t i = 0; i < 256; i++) {
        uint8_t r = 0, g = 0, b = 0;
        if (session.paletteRgb(i, &r, &g, &b)) {
            m_palette[i] = QColor(r, g, b);
        }
    }

    const auto pair = [&session](TtColorPair which, QColor *out) {
        for (int i = 0; i < 2; i++) {
            uint8_t r = 0, g = 0, b = 0;
            if (session.colorRgb(which, i == 1, &r, &g, &b)) {
                out[i] = QColor(r, g, b);
            }
        }
    };
    pair(TT_COLOR_PAIR_NORMAL, m_normal);
    pair(TT_COLOR_PAIR_BOLD, m_bold);
    pair(TT_COLOR_PAIR_BLINK, m_blink);
    pair(TT_COLOR_PAIR_UNDERLINE, m_underline);
    pair(TT_COLOR_PAIR_URL, m_url);
    pair(TT_COLOR_PAIR_REVERSE, m_reverse);

    // The cursor is painted in the normal foreground, which is what upstream
    // does when `VTCursorColor` is absent — and it is absent from the schema,
    // so following the text colour is the only answer that cannot leave an
    // invisible cursor on a reconfigured background. It follows an `OSC 10`
    // for the same reason.
    m_cursor = m_normal[0];

    if (readFlag(session, "terminal.dark_mode", false)) {
        applyDarkPalette();
    }
}

void Theme::applyDarkPalette()
{
    // Painter-only: none of these colours enters the core or QApplication's
    // palette. Explicit SGR foregrounds/backgrounds still win in `resolve`;
    // these are the defaults and attribute pairs a host left unspecified.
    const QColor foreground(0xd4, 0xd4, 0xd4);
    const QColor background(0x1e, 0x1e, 0x1e);
    m_normal[0] = foreground;
    m_normal[1] = background;
    m_bold[0] = QColor(0x56, 0x9c, 0xd6);
    m_bold[1] = background;
    m_blink[0] = QColor(0xf4, 0x47, 0x47);
    m_blink[1] = background;
    m_underline[0] = QColor(0xc5, 0x86, 0xc0);
    m_underline[1] = background;
    m_url[0] = QColor(0x4e, 0xc9, 0xb0);
    m_url[1] = background;
    m_reverse[0] = background;
    m_reverse[1] = foreground;
    m_cursor = foreground;
}

void Theme::applySettings(const Session &session)
{
    readColors(session);

    // The master switch, and the one flag here the core also reads: with it
    // off `SGR 30-37` still lands in the cell and `vtdisp.c:2417` declines to
    // draw with it, so the screen is `color.normal` while the buffer says
    // otherwise. Everything below is this one's business.
    m_ansiColor = readFlag(session, "color.ansi_enabled", m_ansiColor);
    m_boldColor = readFlag(session, "color.bold_enabled", m_boldColor);
    m_blinkColor = readFlag(session, "color.blink_enabled", m_blinkColor);
    m_underlineColor = readFlag(session, "color.underline_enabled", m_underlineColor);
    m_urlColor = readFlag(session, "color.url_enabled", m_urlColor);
    m_reverseColor = readFlag(session, "color.reverse_enabled", m_reverseColor);
    m_useTextColor = readFlag(session, "color.use_text_color", m_useTextColor);
    m_useNormalBg =
        readFlag(session, "color.use_normal_background", m_useNormalBg);
    m_boldFontEnabled = readFlag(session, "color.bold_font", m_boldFontEnabled);
    m_underlineFontEnabled =
        readFlag(session, "color.underline_font", m_underlineFontEnabled);
    m_urlUnderlineEnabled =
        readFlag(session, "color.url_underline", m_urlUnderlineEnabled);
    m_drawResizedFont =
        readFlag(session, "font.draw_resized", m_drawResizedFont);

    const int left = readInt(session, "font.space_left", m_spaceLeft);
    const int right = readInt(session, "font.space_right", m_spaceRight);
    const int top = readInt(session, "font.space_top", m_spaceTop);
    const int bottom = readInt(session, "font.space_bottom", m_spaceBottom);
    if (left != m_spaceLeft || right != m_spaceRight || top != m_spaceTop ||
        bottom != m_spaceBottom) {
        m_spaceLeft = left;
        m_spaceRight = right;
        m_spaceTop = top;
        m_spaceBottom = bottom;
        recomputeMetrics();
    }

    // Win32 exposes four rasterisation requests. Qt has the same default,
    // antialiased and non-antialiased choices; ClearType's subpixel details
    // remain the platform paint engine's decision, so it maps to the explicit
    // antialias request here instead of pretending every desktop is Windows.
    const QString quality = session.setting(QStringLiteral("font.quality"));
    QFont::StyleStrategy strategy = QFont::PreferDefault;
    if (quality == QLatin1String("nonantialiased")) {
        strategy = QFont::NoAntialias;
    } else if (quality == QLatin1String("antialiased") ||
               quality == QLatin1String("cleartype")) {
        strategy = QFont::PreferAntialias;
    }
    if (m_font.styleStrategy() != strategy) {
        QFont font = m_font;
        font.setStyleStrategy(strategy);
        setFont(font);
    }
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
    // Measure the face without the cell advance installed by the previous
    // pass. `applySettings` can change `VTFontSpace` repeatedly, and measuring
    // our own old spacing would make each Apply grow the cell again.
    m_font.setLetterSpacing(QFont::AbsoluteSpacing, 0.0);
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
    m_fontW = qMax(1, qRound(advance));
    m_fontH = qMax(1, qCeil(fm.height()));
    m_cellW = qMax(1, m_fontW + m_spaceLeft + m_spaceRight);
    m_font.setLetterSpacing(QFont::AbsoluteSpacing, m_cellW - advance);

    m_cellH = qMax(1, m_fontH + m_spaceTop + m_spaceBottom);
    m_baseline = qCeil(fm.ascent()) + m_spaceTop;

    m_boldFont = m_font;
    m_boldFont.setBold(true);
}

bool Theme::shouldResizeGlyph(const QString &text, bool bold, int cells) const
{
    if (!m_drawResizedFont || text.isEmpty() || cells <= 0) {
        return false;
    }

    // The selected fixed-width face already puts printable ASCII on the cell
    // grid through the font's letter spacing. Avoid a metrics lookup for the
    // overwhelmingly common path; wide and non-ASCII glyphs are precisely
    // where fallback faces acquire a different natural advance.
    if (cells == 1 && text.size() == 1 && text.at(0).unicode() < 0x80) {
        return false;
    }
    const QFontMetricsF fm(bold ? m_boldFont : m_font);
    const qreal advance = fm.horizontalAdvance(text);
    // `DrawingResizedFont` scales into FontWidth, not CellWidth: the latter
    // includes `VTFontSpace` and stretching through that padding would turn a
    // margin into a wider glyph (`vtdisp.c:2759`).
    const qreal target = cells * m_fontW;
    return advance > 0.0 && qAbs(advance - target) > 1.0;
}

void Theme::resolve(const TtCell &cell, bool selected, bool screenReverse,
                    QColor *fg, QColor *bg, const CellOverride *over) const
{
    // The cell's own attributes, and deliberately *not* a highlight rule's.
    //
    // Upstream's bold, blink and underline each carry a colour pair, so OR-ing
    // a rule's underline in here would make "underline this" repaint the text
    // magenta — the configured `color.underline`. A rule's bold and underline
    // are a mark rather than an SGR attribute: they reach the font and the
    // stroke, through the caller's `paintsBold` / `paintsUnderline`, and the
    // only colours a rule decides are its own.
    const uint32_t attrs = cell.attrs;

    // Upstream composes the attribute with the setting that enables it before
    // looking at anything, so an attribute whose colour is switched off falls
    // through to the normal pair rather than to a disabled-looking one.
    const bool bold = m_boldColor && (attrs & TT_ATTR_BOLD);
    const bool blink = m_blinkColor && (attrs & TT_ATTR_BLINK);
    const bool underline = m_underlineColor && (attrs & TT_ATTR_UNDER);
    const bool url = m_urlColor && (attrs & TT_ATTR_URL);

    bool reverse = selected;
    if (attrs & TT_ATTR_REVERSE) {
        reverse = !reverse;
    }
    if (screenReverse) {
        reverse = !reverse;
    }
    // A rule's reverse is the one attribute of its that *is* about colour, so
    // it joins the count rather than the pair chain — and it comes before the
    // rule's own colours below, so `fg=red, reverse` is red behind the text,
    // which is what `SGR 31` and `SGR 7` together do.
    if (over && (over->attrs & TT_ATTR_REVERSE)) {
        reverse = !reverse;
    }

    // Blink beats bold beats underline beats URL (`vtdisp.c:2449-2514`). Only
    // one pair applies; they do not mix.
    const QColor *pair = nullptr;
    if (blink) {
        pair = m_blink;
    } else if (bold) {
        pair = m_bold;
    } else if (underline) {
        pair = m_underline;
    } else if (url) {
        pair = m_url;
    }

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

    // `CF_USETEXTCOLOR`, after both explicit colours. Some applications assume
    // a black terminal and set white-on-white or black-on-black when Tera
    // Term's configured background is the opposite. Upstream repairs only a
    // same-colour pair whose foreground is black, white or bright white — not
    // every invisible pair (`vtdisp.c:2542`). Under reverse it deliberately
    // uses the configured reverse pair even when that pair's ordinary enable
    // flag is off.
    if (m_useTextColor && m_ansiColor && (attrs & TT_ATTR2_FORE) &&
        (attrs & TT_ATTR2_BACK) && cell.fg == cell.bg &&
        (cell.fg == 0 || cell.fg == 7 || cell.fg == 15)) {
        const QColor *safe = reverse ? m_reverse : m_normal;
        *fg = safe[0];
        *bg = safe[1];
    }

    // And a highlight rule wins over all of it, including the repair above —
    // which tests the *cell's* two indices and would otherwise throw away a
    // colour the user asked for on a cell the host had made invisible.
    if (over) {
        if (over->fg.isValid()) {
            *(reverse ? bg : fg) = over->fg;
        }
        if (over->bg.isValid()) {
            *(reverse ? fg : bg) = over->bg;
        }
    }
}
