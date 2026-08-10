// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "Theme.h"

#include <QFontDatabase>
#include <QFontMetricsF>
#include <QtMath>

#include "Session.h"

namespace {

/// `fg_r,fg_g,fg_b,bg_r,bg_g,bg_b` into the pair upstream stores.
///
/// A short or unparseable value leaves the pair alone rather than throwing it
/// away — the same rule `tt-config`'s reader applies to the file, so a colour
/// that arrives half-written does not paint the screen black.
void readPair(const Session &session, const char *name, QColor *pair)
{
    const QStringList parts = session.setting(QString::fromLatin1(name)).split(QLatin1Char(','));
    if (parts.size() < 6) {
        return;
    }
    int n[6];
    for (int i = 0; i < 6; i++) {
        bool ok = false;
        n[i] = parts.at(i).trimmed().toInt(&ok);
        if (!ok || n[i] < 0 || n[i] > 255) {
            return;
        }
    }
    pair[0] = QColor(n[0], n[1], n[2]);
    pair[1] = QColor(n[3], n[4], n[5]);
}

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

void Theme::applySettings(const Session &session)
{
    // The parser resolves truecolor against this same live table, so the
    // painter must read it from the session too. Reading `ANSIColor` again in
    // Qt would create two parsers for its masking, wrapping and buffer limits.
    for (uint32_t i = 0; i < 256; i++) {
        uint8_t r = 0, g = 0, b = 0;
        if (session.paletteRgb(i, &r, &g, &b)) {
            m_palette[i] = QColor(r, g, b);
        }
    }

    readPair(session, "color.normal", m_normal);
    readPair(session, "color.bold", m_bold);
    readPair(session, "color.blink", m_blink);
    readPair(session, "color.underline", m_underline);
    readPair(session, "color.url", m_url);
    readPair(session, "color.reverse", m_reverse);

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

    // The cursor is painted in the normal foreground, which is what upstream
    // does when `VTCursorColor` is absent — and it is absent from the schema,
    // so following the text colour is the only answer that cannot leave an
    // invisible cursor on a reconfigured background.
    m_cursor = m_normal[0];
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
    const qreal target = cells * m_cellW;
    return advance > 0.0 && qAbs(advance - target) > 1.0;
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
    const bool url = m_urlColor && (attrs & TT_ATTR_URL);

    bool reverse = selected;
    if (attrs & TT_ATTR_REVERSE) {
        reverse = !reverse;
    }
    if (screenReverse) {
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
}
