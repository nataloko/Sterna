// Tabs whose pages can be visible in one, two or four panels.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QVector>
#include <QWidget>

class QTabBar;
class PaneFrame;

enum class PanelLayout { Single = 1, Two = 2, Four = 4 };

/// The window's lightweight tab bar and equal-sized visible panel grid.
///
/// Pages remain owned by their caller. This widget owns only their placement:
/// every page stays in tab order, at most four are parented into visible pane
/// frames, and the rest are hidden children whose sessions may keep running.
class PanelContainer : public QWidget {
    Q_OBJECT

public:
    enum class ConnectionKind { Serial, Ssh, Telnet, Shell };
    Q_ENUM(ConnectionKind)

    explicit PanelContainer(QWidget *parent = nullptr);

    int count() const { return m_pages.size(); }
    QWidget *widget(int index) const;
    int indexOf(QWidget *page) const { return m_pages.indexOf(page); }

    QWidget *currentWidget() const { return m_current; }
    int currentIndex() const { return indexOf(m_current); }
    void setCurrentWidget(QWidget *page);
    void setCurrentIndex(int index);

    int addPage(QWidget *page, const QString &title, int preferredPanel = -1);
    QWidget *removePage(int index);
    void setTabText(int index, const QString &title);
    void setTabToolTip(int index, const QString &toolTip);
    void setTabsClosable(bool closable);
    bool tabsClosable() const;
    QTabBar *tabBar() const { return m_tabs; }

    PanelLayout layoutMode() const { return m_layout; }
    void setLayoutMode(PanelLayout layout);
    int panelCount() const { return static_cast<int>(m_layout); }
    QWidget *pageAtPanel(int panel) const;
    int panelOf(QWidget *page) const;
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
    void assign(int panel, QWidget *page);
    void arrangeFrames();
    void updateActiveFrames();
    QWidget *firstHiddenPage() const;

    QTabBar *m_tabs = nullptr;
    QWidget *m_gridWidget = nullptr;
    QVector<PaneFrame *> m_frames;
    QVector<QWidget *> m_pages;
    QWidget *m_current = nullptr;
    PanelLayout m_layout = PanelLayout::Single;
    bool m_changingTabs = false;
};

