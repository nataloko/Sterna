// The terminal's size, shown while somebody is changing it.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QString>
#include <QWidget>

class QTimer;
class Theme;

/// A short-lived box in the middle of the terminal reading `COLSxROWS`.
///
/// Upstream shows the same two numbers, as a tooltip beside the corner being
/// dragged, created on `WM_ENTERSIZEMOVE` and destroyed on `WM_EXITSIZEMOVE`
/// (`sizetip.c:111`, `vtwin.cpp:3111`). Neither half of that survives the port:
/// Wayland sends no such pair of events, so there is nothing to bracket, and a
/// client cannot place a window near the pointer — `QWidget::move()` is
/// silently ignored there. What is left is a child of the terminal and a timer,
/// which is what every terminal on this desktop does anyway.
///
/// **It floats rather than taking layout space**, for the reason the find bar
/// does (`TerminalView::positionFindBar`): a widget in the page's layout would
/// take a row from the grid, and taking a row is a resize — so a box that
/// reports resizes would cause one, and report that.
class SizeIndicator : public QWidget {
    Q_OBJECT

public:
    /// Borrows the terminal's theme, so the box is in the terminal's own font
    /// and colours and cannot disagree with the text under it.
    SizeIndicator(const Theme &theme, QWidget *parent);

    /// Show `text` and take it away again after `ms` of quiet. Called again
    /// while it is up, the clock starts over — which is what makes a drag show
    /// one box that counts rather than a queue of them.
    void flash(const QString &text, int ms = 1000);

    /// What it is showing. Public for the test, which has no other way to read
    /// two numbers out of a painted box.
    QString text() const { return m_text; }

    QSize sizeHint() const override;

protected:
    void paintEvent(QPaintEvent *event) override;

private:
    const Theme &m_theme;
    QString m_text;
    QTimer *m_timer = nullptr;
};
