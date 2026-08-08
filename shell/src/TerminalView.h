// The grid: painting, keyboard, mouse, selection.
//
// Copyright (c) the termitta authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QPoint>
#include <QWidget>

#include "Theme.h"
#include "termitta.h"

class Session;

/// The terminal screen.
///
/// A plain `QWidget` with a `QPainter`, no GPU. The measured baseline on the
/// Qt the desktop actually runs is a full 80x24 repaint in 3.9 ms — about 40x
/// what a 115200 baud link can dirty — so the scarce resource here is not fill
/// rate, and spending a GPU context and 60 MB on it would be spending it on
/// the wrong thing. See `PLAN.md`.
class TerminalView : public QWidget {
    Q_OBJECT

public:
    explicit TerminalView(Session *session, QWidget *parent = nullptr);

    Theme &theme() { return m_theme; }
    const Theme &theme() const { return m_theme; }
    /// Re-measure the font and re-fit the terminal to the window.
    void applyFont(const QFont &font);

    QSize sizeHint() const override;
    /// The pixel size this many cells needs, at the current font.
    QSize sizeForCells(int cols, int rows) const;

    /// Copy the selection, if there is one.
    void copySelection() const;
    /// Paste the system clipboard.
    void pasteClipboard();
    bool hasSelection() const { return m_hasSelection; }

public slots:
    /// Scroll the view back by `offset` lines; 0 is the live screen.
    void setViewOffset(int offset);

signals:
    /// The viewport moved, or the history grew. A scrollbar watches this
    /// rather than assuming its own last write is still current — the core
    /// moves the offset itself to keep a scrolled-back view on the same lines.
    void viewChanged();

protected:
    void paintEvent(QPaintEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void mouseDoubleClickEvent(QMouseEvent *event) override;
    void wheelEvent(QWheelEvent *event) override;
    void focusInEvent(QFocusEvent *event) override;
    void focusOutEvent(QFocusEvent *event) override;

private:
    /// Cell under a widget position, clamped to the grid.
    QPoint cellAt(const QPointF &pos) const;
    bool isSelected(int x, int y) const;
    QString selectedText() const;
    void clearSelection();
    /// Re-fit the terminal to the widget, in whole cells.
    void refit();

    Session *m_session;
    Theme m_theme;

    // Selection is a frontend concept — the core only has to support it — and
    // it lives on the visible screen because there is no scrollback viewport
    // to select into yet.
    bool m_hasSelection = false;
    bool m_selecting = false;
    QPoint m_selAnchor;
    QPoint m_selHead;
};
