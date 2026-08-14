// Connections shown one at a time behind a tab bar, or all at once as tiles.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QVector>
#include <QWidget>

class QGridLayout;
class QTabBar;
class PaneFrame;

/// How this window shows its connections. **The two are exclusive**: tiles are
/// not a view *onto* the tabs, they replace them.
enum class PanelLayout {
    /// One connection visible, the rest behind a tab bar.
    Single,
    /// Every connection visible, in a grid that fits their number, and no tab
    /// bar at all.
    Tiled,
};

/// The window's tab bar and its tile grid, which are never both on screen.
///
/// Pages remain owned by their caller. This widget owns only their placement.
/// In `Single` that is one visible page and a tab bar; in `Tiled` it is every
/// page in tab order, one per cell, with no tab bar and **no hidden page** —
/// which is the whole point, since a session nobody can see and nobody can
/// reach was the confusing half of the old design.
class PanelContainer : public QWidget {
    Q_OBJECT

public:
    enum class ConnectionKind { Serial, Ssh, Telnet, Shell };
    Q_ENUM(ConnectionKind)

    explicit PanelContainer(QWidget *parent = nullptr);

    /// One terminal's preferred size, whatever the tile count. A layout change
    /// divides the existing client area; it never asks the top-level window to
    /// grow with the number of connections.
    QSize sizeHint() const override;

    int count() const { return m_pages.size(); }
    QWidget *widget(int index) const;
    int indexOf(QWidget *page) const { return m_pages.indexOf(page); }

    QWidget *currentWidget() const { return m_current; }
    int currentIndex() const { return indexOf(m_current); }
    void setCurrentWidget(QWidget *page);
    void setCurrentIndex(int index);

    int addPage(QWidget *page, const QString &title);
    QWidget *removePage(int index);
    void setTabText(int index, const QString &title);
    void setTabToolTip(int index, const QString &toolTip);
    void setTabsClosable(bool closable);
    bool tabsClosable() const;
    QTabBar *tabBar() const { return m_tabs; }

    PanelLayout layoutMode() const { return m_layout; }
    void setLayoutMode(PanelLayout layout);

    /// How many cells the grid has, connections plus the spare one when the
    /// rectangle does not come out even. Always at least 1.
    int tileCount() const;
    /// Columns in the current grid. `tileCount()` and this give the shape.
    int tileColumns() const;
    QWidget *pageAtPanel(int panel) const;
    int panelOf(QWidget *page) const;
    /// The spare cell's index, or -1 when the grid is exactly full. It is
    /// always the one immediately after the last connection.
    int firstEmptyPanel() const;
    QVector<QWidget *> visiblePages() const;

signals:
    void currentChanged(QWidget *page);
    void closeRequested(QWidget *page);
    void emptyConnectionRequested(int panel,
                                  PanelContainer::ConnectionKind kind);
    /// Page assignment or panel geometry changed. Child layout is queued by
    /// Qt, so consumers which inspect sizes should do so on the next turn.
    void visiblePagesChanged();

protected:
    void resizeEvent(QResizeEvent *event) override;

private:
    friend class PaneFrame;

    void activateFromPanel(int panel);
    void requestConnection(int panel, ConnectionKind kind);
    /// Put every page where the current mode says it goes, and hide the rest of
    /// the pool. The one place the grid is decided.
    void rebuild();
    /// Frame `index`, created if this window has never needed one that far out.
    /// The pool only ever grows: a destroyed frame stays alive for a turn and
    /// answers `findChild("panelFrame0")` first, which is the same trap
    /// `QToolBar::clear()` sets.
    PaneFrame *frameAt(int index);

    QTabBar *m_tabs = nullptr;
    QWidget *m_gridWidget = nullptr;
    QGridLayout *m_grid = nullptr;
    QVector<PaneFrame *> m_frames;
    QVector<QWidget *> m_pages;
    QWidget *m_current = nullptr;
    PanelLayout m_layout = PanelLayout::Single;
    bool m_changingTabs = false;
};
