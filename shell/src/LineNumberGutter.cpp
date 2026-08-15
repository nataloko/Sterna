// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "LineNumberGutter.h"

#include <QPainter>

#include "Session.h"
#include "Theme.h"

LineNumberGutter::LineNumberGutter(const Session *session, const Theme &theme,
                                  QWidget *parent)
    : QWidget(parent)
    , m_session(session)
    , m_theme(theme)
{
    setObjectName(QStringLiteral("lineNumberGutter"));
    // It never wants a click: the terminal beside it owns selection, and a
    // gutter that took focus would be a way to lose the keyboard by aiming
    // badly. Wheel events still reach the view because they are not this
    // widget's to consume.
    setFocusPolicy(Qt::NoFocus);
    setAttribute(Qt::WA_TransparentForMouseEvents);
    updateMetrics();
}

int LineNumberGutter::widthForDigits() const
{
    // One column past the digits, holding the padding and the rule. The digits
    // themselves are as wide as the terminal's cells because they are drawn in
    // the terminal's font, which `Theme` has already given letter spacing that
    // makes one glyph exactly one cell.
    return (m_digits + 1) * m_theme.cellWidth();
}

void LineNumberGutter::setDigits(int digits)
{
    const int clamped = qBound(1, digits, 10);
    if (clamped == m_digits) {
        return;
    }
    m_digits = clamped;
    updateMetrics();
}

void LineNumberGutter::updateMetrics()
{
    // Fixed, so the layout beside it cannot stretch or squeeze the column and
    // put the digits somewhere other than under each other.
    setFixedWidth(widthForDigits());
    updateGeometry();
    update();
}

QSize LineNumberGutter::sizeHint() const
{
    return QSize(widthForDigits(), m_theme.cellHeight());
}

void LineNumberGutter::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    const int cw = m_theme.cellWidth();
    const int ch = m_theme.cellHeight();

    p.fillRect(rect(), m_theme.defaultBackground());

    const QColor ink = m_theme.lineNumberColor();
    p.setPen(ink);
    p.setFont(m_theme.font());

    // The rows the terminal is showing, which is what `Session::lineAt`
    // answers for — so scrolling back moves the numbers with the text and no
    // offset is tracked here.
    const int rows = m_session->rows();
    for (int y = 0; y < rows; y++) {
        const int top = y * ch;
        if (top >= height()) {
            break;
        }
        // 1-based: the core counts the first line the host printed as 0, and
        // no editor, pager or compiler anybody has met counts that way.
        const QString text = QString::number(m_session->lineAt(y) + 1);
        // Right-aligned by cell arithmetic rather than by measuring the string
        // or by handing Qt a rectangle: `Theme` has given this font absolute
        // letter spacing that makes one glyph exactly one cell, so a digit's
        // column is known without asking, and drawing from `baseline()` puts
        // the numbers on the same baseline as the text beside them — which an
        // `AlignVCenter` rectangle would miss by however much the font's
        // ascent and descent differ.
        //
        // A number longer than its field gets a negative column and spills
        // leftwards, which is the documented overflow.
        p.drawText(QPoint((m_digits - text.size()) * cw + m_theme.textOffsetX(),
                          top + m_theme.baseline()),
                   text);
    }

    // A hairline rather than a gap, so the eye has an edge to run down. In the
    // same dim colour as the digits: it is the same piece of furniture.
    const int rule = width() - 1;
    p.drawLine(rule, 0, rule, height());
}
