// The quick-button panel's resize edge.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QWidget>

/// The handle between the terminals and the quick-button panel.
///
/// **This is a splitter handle that does not split anything**, and that is the
/// whole point of it existing. A `QSplitter` — and `QMainWindow`'s dock
/// separator, which is what this replaces — divides a fixed total, so every
/// pixel the panel gains is a pixel the terminal loses. The terminal is fitted
/// to whatever width is left in whole cells, so a few pixels either way is a
/// column, and a column is a real `Grid::resize`, which truncates every line
/// it shortens in the page *and* the scrollback. Dragging a panel is not a
/// gesture anybody expects to destroy text.
///
/// So this widget reports a drag and decides nothing. `MainWindow` owns the
/// arithmetic, because the answer depends on the screen's work area, the
/// window's frame and whether the window is maximised — none of which a handle
/// should know about. It paints as `CE_Splitter` and carries a `SplitHCursor`
/// so that it still reads as the thing it looks like.
class QuickButtonGrip : public QWidget {
    Q_OBJECT

public:
    explicit QuickButtonGrip(QWidget *parent = nullptr);

    QSize sizeHint() const override;

signals:
    /// The mouse went down on the handle. The window records what the panel
    /// and the window measured before anything moved.
    void resizeStarted();
    /// `delta` is measured from the press, not from the last move, so a drag
    /// that wanders back to where it began asks for the width it began with.
    /// Positive is wider: the handle is to the *left* of the panel, so
    /// dragging left grows it.
    void resizeMoved(int delta);
    /// The button came back up. The width is written to the settings here and
    /// not on every move — the file is not a drag's undo history.
    void resizeFinished();

protected:
    void paintEvent(QPaintEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;

private:
    bool m_dragging = false;
    /// Global, because the widget moves under the pointer as the drag runs —
    /// a local x would measure against a moving origin and the panel would
    /// accelerate away from the mouse.
    int m_pressX = 0;
};
