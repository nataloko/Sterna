// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "QuickButtonGrip.h"

#include <QMouseEvent>
#include <QPainter>
#include <QStyle>
#include <QStyleOption>

QuickButtonGrip::QuickButtonGrip(QWidget *parent) : QWidget(parent)
{
    setObjectName(QStringLiteral("quickButtonGrip"));
    setCursor(Qt::SplitHCursor);
    // Fixed across, free along: the handle is as thick as the style's splitter
    // and as tall as the row it sits in.
    setSizePolicy(QSizePolicy::Fixed, QSizePolicy::Expanding);
}

QSize QuickButtonGrip::sizeHint() const
{
    // The style's own number, so the handle is the width the desktop's other
    // splitters are. Floored at three because a theme answering 0 or 1 leaves
    // nothing to aim a pointer at, and the pointer is the only way in.
    const int width =
        qMax(3, style()->pixelMetric(QStyle::PM_SplitterWidth, nullptr, this));
    return QSize(width, width);
}

void QuickButtonGrip::paintEvent(QPaintEvent *)
{
    QPainter painter(this);
    QStyleOption option;
    option.initFrom(this);
    option.rect = rect();
    // `State_Horizontal` on a splitter handle means the handle *moves*
    // horizontally, which is this one. Leaving it off draws the grip's dots
    // along the wrong axis on the styles that draw dots at all.
    option.state |= QStyle::State_Horizontal;
    style()->drawControl(QStyle::CE_Splitter, &option, &painter, this);
}

void QuickButtonGrip::mousePressEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton) {
        QWidget::mousePressEvent(event);
        return;
    }
    m_dragging = true;
    m_pressX = int(event->globalPosition().x());
    emit resizeStarted();
    event->accept();
}

void QuickButtonGrip::mouseMoveEvent(QMouseEvent *event)
{
    if (!m_dragging) {
        QWidget::mouseMoveEvent(event);
        return;
    }
    // Left is wider. The panel is to the right of this handle, so the pointer
    // moving toward the terminal is the panel growing over it.
    emit resizeMoved(m_pressX - int(event->globalPosition().x()));
    event->accept();
}

void QuickButtonGrip::mouseReleaseEvent(QMouseEvent *event)
{
    if (!m_dragging || event->button() != Qt::LeftButton) {
        QWidget::mouseReleaseEvent(event);
        return;
    }
    m_dragging = false;
    emit resizeFinished();
    event->accept();
}
