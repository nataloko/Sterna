// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "PanelContainer.h"

#include <QEvent>
#include <QFrame>
#include <QGridLayout>
#include <QLabel>
#include <QMouseEvent>
#include <QPushButton>
#include <QResizeEvent>
#include <QSignalBlocker>
#include <QStackedLayout>
#include <QTabBar>
#include <QVBoxLayout>

/// One cell: either a page or the four connect buttons. The frame is
/// deliberately only presentation; tab order and cell assignment stay in
/// PanelContainer, and the connection's name and state are on the page's own
/// status strip rather than on a header here.
class PaneFrame final : public QFrame {
public:
    PaneFrame(PanelContainer *owner, int panel)
        : QFrame(owner)
        , m_owner(owner)
        , m_panel(panel)
    {
        setObjectName(QStringLiteral("panelFrame%1").arg(panel));
        // The grid spacing separates panes. A styled frame changes its width
        // when the platform style is first polished, which makes a pre-show
        // size hint two pixels shorter than the same hint after show.
        setFrameShape(QFrame::NoFrame);
        setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Ignored);

        auto *outer = new QVBoxLayout(this);
        outer->setContentsMargins(1, 1, 1, 1);
        outer->setSpacing(0);

        m_content = new QWidget(this);
        m_stack = new QStackedLayout(m_content);
        m_stack->setContentsMargins(0, 0, 0, 0);

        m_empty = new QWidget(m_content);
        m_empty->setObjectName(QStringLiteral("emptyPanel%1").arg(panel));
        auto *emptyLayout = new QVBoxLayout(m_empty);
        emptyLayout->addStretch();
        auto *caption = new QLabel(tr("New connection"), m_empty);
        caption->setAlignment(Qt::AlignCenter);
        emptyLayout->addWidget(caption);
        const auto button = [this, emptyLayout](const QString &text,
                                                const QString &name,
                                                PanelContainer::ConnectionKind kind) {
            auto *out = new QPushButton(text, m_empty);
            out->setObjectName(name + QString::number(m_panel));
            out->setMaximumWidth(180);
            emptyLayout->addWidget(out, 0, Qt::AlignHCenter);
            connect(out, &QPushButton::clicked, m_owner,
                    [this, kind] { m_owner->requestConnection(m_panel, kind); });
        };
        button(tr("Serial"), QStringLiteral("panelSerial"),
               PanelContainer::ConnectionKind::Serial);
        button(tr("SSH"), QStringLiteral("panelSsh"),
               PanelContainer::ConnectionKind::Ssh);
        button(tr("Telnet"), QStringLiteral("panelTelnet"),
               PanelContainer::ConnectionKind::Telnet);
        button(tr("Local shell"), QStringLiteral("panelShell"),
               PanelContainer::ConnectionKind::Shell);
        emptyLayout->addStretch();
        m_stack->addWidget(m_empty);
        outer->addWidget(m_content, 1);
        showEmpty();
    }

    QWidget *page() const { return m_page; }

    QSize sizeHintFor(QWidget *page) const
    {
        QSize out = page ? page->sizeHint() : QSize(640, 400);
        const QMargins margins = layout()->contentsMargins();
        out.rwidth() += margins.left() + margins.right() + frameWidth() * 2;
        out.rheight() += margins.top() + margins.bottom() + frameWidth() * 2;
        return out;
    }

    void setPage(QWidget *page)
    {
        if (m_page == page) {
            return;
        }
        takePage();
        m_page = page;
        if (!m_page) {
            showEmpty();
            return;
        }
        m_page->setParent(m_content);
        m_stack->addWidget(m_page);
        watchPage(true);
        m_stack->setCurrentWidget(m_page);
        m_page->show();
    }

    QWidget *takePage()
    {
        QWidget *out = m_page;
        if (!out) {
            return nullptr;
        }
        watchPage(false);
        m_stack->removeWidget(out);
        out->hide();
        out->setParent(m_owner);
        m_page = nullptr;
        showEmpty();
        return out;
    }

protected:
    bool eventFilter(QObject *watched, QEvent *event) override
    {
        if ((event->type() == QEvent::MouseButtonPress
             || event->type() == QEvent::FocusIn)
            && m_watched.contains(watched)) {
            m_owner->activateFromPanel(m_panel);
        }
        return QFrame::eventFilter(watched, event);
    }

private:
    void showEmpty()
    {
        m_stack->setCurrentWidget(m_empty);
        m_empty->show();
    }

    void watchPage(bool watch)
    {
        if (watch) {
            m_watched = {m_page};
            const auto children = m_page->findChildren<QWidget *>();
            for (QWidget *child : children) {
                m_watched.append(child);
            }
            for (QObject *object : m_watched) {
                object->installEventFilter(this);
            }
        } else {
            for (QObject *object : m_watched) {
                object->removeEventFilter(this);
            }
            m_watched.clear();
        }
    }

    PanelContainer *m_owner = nullptr;
    int m_panel = 0;
    QWidget *m_content = nullptr;
    QStackedLayout *m_stack = nullptr;
    QWidget *m_empty = nullptr;
    QWidget *m_page = nullptr;
    QVector<QObject *> m_watched;
};

namespace {
/// The grid for `n` connections: the smallest square-ish rectangle that holds
/// them. 1, 2, 3-4, 5-6, 7-9 give 1x1, 1x2, 2x2, 2x3, 3x3, and it keeps going
/// rather than capping — a window full of postage stamps is the user's
/// business, and one View-menu click undoes it.
int columnsFor(int n)
{
    int cols = 1;
    while (cols * cols < n) {
        cols++;
    }
    return cols;
}

/// How many cells the last row has spare. 0 means the rectangle came out even
/// and there is no connect cell at all — which is the case at 1, 2, 4, 6 and 9.
int leftoverFor(int n)
{
    const int cols = columnsFor(n);
    const int last = n % cols;
    return last == 0 ? 0 : cols - last;
}
} // namespace

PanelContainer::PanelContainer(QWidget *parent)
    : QWidget(parent)
{
    auto *outer = new QVBoxLayout(this);
    outer->setContentsMargins(0, 0, 0, 0);
    outer->setSpacing(0);

    m_tabs = new QTabBar(this);
    m_tabs->setObjectName(QStringLiteral("connectionTabBar"));
    // Not `setAutoHide`: auto-hide knows about the tab count and cannot know
    // about the layout mode, and tiles must hide the bar however many there
    // are. `rebuild()` owns the answer.
    m_tabs->setDocumentMode(true);
    m_tabs->setMovable(true);
    m_tabs->setExpanding(false);
    m_tabs->hide();
    outer->addWidget(m_tabs);

    m_gridWidget = new QWidget(this);
    m_gridWidget->setObjectName(QStringLiteral("panelGrid"));
    m_grid = new QGridLayout(m_gridWidget);
    m_grid->setContentsMargins(0, 0, 0, 0);
    m_grid->setSpacing(2);
    outer->addWidget(m_gridWidget, 1);

    rebuild();

    connect(m_tabs, &QTabBar::currentChanged, this, [this](int index) {
        if (!m_changingTabs) {
            setCurrentIndex(index);
        }
    });
    connect(m_tabs, &QTabBar::tabCloseRequested, this, [this](int index) {
        if (QWidget *page = widget(index)) {
            emit closeRequested(page);
        }
    });
    connect(m_tabs, &QTabBar::tabMoved, this, [this](int from, int to) {
        if (!m_changingTabs && from >= 0 && from < m_pages.size() && to >= 0
            && to < m_pages.size()) {
            m_pages.move(from, to);
            // Tiles are tab order, so dragging a tab in Single mode decides
            // which tile a connection gets when tiles come back.
            rebuild();
            emit visiblePagesChanged();
        }
    });
}

QWidget *PanelContainer::widget(int index) const
{
    return index >= 0 && index < m_pages.size() ? m_pages[index] : nullptr;
}

int PanelContainer::tileCount() const
{
    if (m_layout == PanelLayout::Single) {
        return 1;
    }
    const int n = m_pages.size();
    return qMax(1, n + (leftoverFor(n) > 0 ? 1 : 0));
}

int PanelContainer::tileColumns() const
{
    return m_layout == PanelLayout::Single ? 1 : columnsFor(m_pages.size());
}

QSize PanelContainer::sizeHint() const
{
    // Frame 0 always exists — the constructor's `rebuild()` makes it — so this
    // never creates a widget from inside a const measurement.
    const int panel = qBound(0, panelOf(m_current), m_frames.size() - 1);
    PaneFrame *frame = m_frames[panel];
    frame->ensurePolished();
    m_tabs->ensurePolished();
    QSize out = frame->sizeHintFor(m_current);
    // Only when the bar is actually on screen. Reserving its height in tiled
    // mode would cost every tile a row for chrome that is not there.
    if (!m_tabs->isHidden()) {
        out.rheight() += m_tabs->sizeHint().height();
    }
    // QMainWindow's menu chrome grows when its native layout is first shown.
    // Keep a sub-cell remainder so an exact N-row size hint does not refit to
    // N-1 on that first layout. Measured against Qt 6.11.1 and 6.4.2 with
    // `render_test`'s configured-size case, which is what moves if this is
    // wrong — see the note on that test.
    out.rheight() += 3;
    return out;
}

void PanelContainer::setCurrentIndex(int index) { setCurrentWidget(widget(index)); }

void PanelContainer::setCurrentWidget(QWidget *page)
{
    if (!page || !m_pages.contains(page)) {
        return;
    }

    const bool currentDidChange = m_current != page;
    m_current = page;
    {
        const QSignalBlocker block(m_tabs);
        m_tabs->setCurrentIndex(indexOf(page));
    }
    // In Single the one pane has to change what it holds. In Tiled every page
    // is already on screen, so only the marker moves — there is no eviction
    // and nothing to reassign.
    if (m_layout == PanelLayout::Single && currentDidChange) {
        rebuild();
        emit visiblePagesChanged();
    }
    if (currentDidChange) {
        emit currentChanged(page);
    }
}

int PanelContainer::addPage(QWidget *page, const QString &title)
{
    if (!page || m_pages.contains(page)) {
        return indexOf(page);
    }
    page->hide();
    page->setParent(this);
    m_pages.append(page);
    m_changingTabs = true;
    const int index = m_tabs->addTab(title);
    m_changingTabs = false;

    // No `preferredPanel`: a new connection appends to tab order, and in Tiled
    // that index *is* the spare cell it was started from. There is nothing left
    // for a caller to choose.
    setCurrentWidget(page);
    rebuild();
    emit visiblePagesChanged();
    return index;
}

QWidget *PanelContainer::removePage(int index)
{
    QWidget *page = widget(index);
    if (!page) {
        return nullptr;
    }
    const bool wasCurrent = page == m_current;
    for (PaneFrame *frame : m_frames) {
        if (frame->page() == page) {
            frame->takePage();
        }
    }

    m_changingTabs = true;
    m_tabs->removeTab(index);
    m_changingTabs = false;
    m_pages.removeAt(index);
    page->hide();
    page->setParent(this);

    if (wasCurrent) {
        // Whoever inherited the removed index, or the last page if it was the
        // last index. No hidden-page refill: in Tiled nothing is hidden, and in
        // Single the one pane simply shows whatever becomes current.
        m_current = nullptr;
        if (QWidget *next = widget(qMin(index, m_pages.size() - 1))) {
            setCurrentWidget(next);
        }
    } else {
        const QSignalBlocker block(m_tabs);
        m_tabs->setCurrentIndex(currentIndex());
    }
    rebuild();
    emit visiblePagesChanged();
    return page;
}

void PanelContainer::setTabText(int index, const QString &title)
{
    if (widget(index)) {
        m_tabs->setTabText(index, title);
    }
}

void PanelContainer::setTabToolTip(int index, const QString &toolTip)
{
    if (widget(index)) {
        m_tabs->setTabToolTip(index, toolTip);
    }
}

void PanelContainer::setTabsClosable(bool closable)
{
    m_tabs->setTabsClosable(closable);
}

bool PanelContainer::tabsClosable() const { return m_tabs->tabsClosable(); }

void PanelContainer::setLayoutMode(PanelLayout layout)
{
    if (layout == m_layout) {
        return;
    }
    m_layout = layout;
    rebuild();
    emit visiblePagesChanged();
}

QWidget *PanelContainer::pageAtPanel(int panel) const
{
    if (panel < 0 || panel >= tileCount()) {
        return nullptr;
    }
    if (m_layout == PanelLayout::Single) {
        return m_current;
    }
    return widget(panel);
}

int PanelContainer::panelOf(QWidget *page) const
{
    if (!page) {
        return -1;
    }
    if (m_layout == PanelLayout::Single) {
        return page == m_current ? 0 : -1;
    }
    return indexOf(page);
}

int PanelContainer::firstEmptyPanel() const
{
    if (m_layout == PanelLayout::Single) {
        return m_current ? -1 : 0;
    }
    return leftoverFor(m_pages.size()) > 0 || m_pages.isEmpty()
               ? m_pages.size()
               : -1;
}

QVector<QWidget *> PanelContainer::visiblePages() const
{
    if (m_layout == PanelLayout::Single) {
        return m_current ? QVector<QWidget *>{m_current} : QVector<QWidget *>{};
    }
    return m_pages;
}

void PanelContainer::resizeEvent(QResizeEvent *event)
{
    QWidget::resizeEvent(event);
    emit visiblePagesChanged();
}

void PanelContainer::activateFromPanel(int panel)
{
    if (QWidget *page = pageAtPanel(panel)) {
        setCurrentWidget(page);
    }
}

void PanelContainer::requestConnection(int panel, ConnectionKind kind)
{
    emit emptyConnectionRequested(panel, kind);
}

PaneFrame *PanelContainer::frameAt(int index)
{
    while (m_frames.size() <= index) {
        auto *frame = new PaneFrame(this, m_frames.size());
        frame->hide();
        m_frames.append(frame);
    }
    return m_frames[index];
}

void PanelContainer::rebuild()
{
    const int tiles = tileCount();
    const int cols = tileColumns();
    const int spare = firstEmptyPanel();

    // Two passes, and the order is load-bearing. Every frame that is not
    // already holding the page it should hold gives that page up *first*, so
    // that by the time anything is assigned no page is still parented into a
    // stack it is about to leave. Assigning straight away would leave the old
    // frame's `QStackedLayout` pointing at a widget somebody else had taken.
    for (int i = 0; i < m_frames.size(); i++) {
        if (m_frames[i]->page() != (i < tiles ? pageAtPanel(i) : nullptr)) {
            m_frames[i]->takePage();
        }
        m_grid->removeWidget(m_frames[i]);
    }

    for (int i = 0; i < tiles; i++) {
        PaneFrame *frame = frameAt(i);
        frame->setPage(pageAtPanel(i));
        const int row = i / cols;
        const int column = i % cols;
        // The spare cell swallows the rest of its row, so a grid that does not
        // come out even has a wider connect cell rather than a blank hole. Only
        // ever one such cell: it is always the last.
        const int span = i == spare ? qMax(1, cols - column) : 1;
        m_grid->addWidget(frame, row, column, 1, span);
        frame->show();
    }
    // Grown but not needed this time: out of the grid and out of sight. Never
    // destroyed — a frame that is still dying answers `findChild` first.
    for (int i = tiles; i < m_frames.size(); i++) {
        m_frames[i]->hide();
    }

    const int rows = (tiles + cols - 1) / cols;
    for (int row = 0; row < m_grid->rowCount(); row++) {
        m_grid->setRowStretch(row, row < rows ? 1 : 0);
    }
    for (int column = 0; column < m_grid->columnCount(); column++) {
        m_grid->setColumnStretch(column, column < cols ? 1 : 0);
    }

    m_tabs->setVisible(m_layout == PanelLayout::Single && m_pages.size() > 1);
}
