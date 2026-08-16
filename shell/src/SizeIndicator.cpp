// The terminal's size, shown while somebody is changing it.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "SizeIndicator.h"

#include <QFontMetrics>
#include <QPainter>
#include <QTimer>

#include "Theme.h"

namespace {
/// Room around the numbers, in cells. Measured in the terminal's own units so
/// the box keeps its proportions at every font size and scale factor, which is
/// the same reason the gutter counts its width in cells.
constexpr int kPadCellsX = 1;
constexpr int kPadCellsY = 1;
} // namespace

SizeIndicator::SizeIndicator(const Theme &theme, QWidget *parent)
    : QWidget(parent)
    , m_theme(theme)
    , m_timer(new QTimer(this))
{
    setObjectName(QStringLiteral("sizeIndicator"));
    // It reports; it is not a control. Letting it take clicks would put a dead
    // spot in the middle of the terminal for a second after every resize.
    setAttribute(Qt::WA_TransparentForMouseEvents);
    setFocusPolicy(Qt::NoFocus);
    hide();
    m_timer->setSingleShot(true);
    connect(m_timer, &QTimer::timeout, this, &QWidget::hide);
}

void SizeIndicator::flash(const QString &text, int ms)
{
    m_text = text;
    // Before `show()`: the parent places this from `sizeHint`, and a hint taken
    // from the previous text would centre the old box and paint the new one
    // into it.
    updateGeometry();
    show();
    raise();
    update();
    m_timer->start(ms);
}

QSize SizeIndicator::sizeHint() const
{
    const QFontMetrics metrics(m_theme.font());
    return QSize(metrics.horizontalAdvance(m_text) + 2 * kPadCellsX * m_theme.cellWidth(),
                 metrics.height() + 2 * kPadCellsY * m_theme.cellHeight());
}

void SizeIndicator::paintEvent(QPaintEvent *)
{
    QPainter p(this);
    p.setRenderHint(QPainter::Antialiasing);
    // The terminal's colours, the other way up. A box in the foreground colour
    // reads as part of the terminal rather than as part of the desktop, and it
    // stays legible through a dark mode, an `OSC 11` and the disconnected
    // shade without knowing that any of them exist.
    const QColor ink = m_theme.defaultBackground();
    const QColor field = m_theme.defaultForeground();
    const int radius = m_theme.cellWidth() / 2;
    p.setPen(Qt::NoPen);
    p.setBrush(field);
    p.drawRoundedRect(rect(), radius, radius);
    p.setPen(ink);
    p.setFont(m_theme.font());
    p.drawText(rect(), Qt::AlignCenter, m_text);
}
