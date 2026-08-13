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

/// One visible slot: a connection title over either a page or four connect
/// buttons. The frame is deliberately only presentation; tab order and slot
/// assignment stay in PanelContainer.
class PaneFrame final : public QFrame {
public:
    PaneFrame(PanelContainer *owner, int panel)
        : QFrame(owner)
        , m_owner(owner)
        , m_panel(panel)
    {
        setObjectName(QStringLiteral("panelFrame%1").arg(panel));
        setFrameShape(QFrame::StyledPanel);
        setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Ignored);

        auto *outer = new QVBoxLayout(this);
        outer->setContentsMargins(1, 1, 1, 1);
        outer->setSpacing(0);

        m_header = new QLabel(tr("New connection"), this);
        m_header->setObjectName(QStringLiteral("panelHeader%1").arg(panel));
        m_header->setAutoFillBackground(true);
        m_header->setContentsMargins(6, 2, 6, 2);
        m_header->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
        m_header->installEventFilter(this);
        outer->addWidget(m_header);

        m_content = new QWidget(this);
        m_stack = new QStackedLayout(m_content);
        m_stack->setContentsMargins(0, 0, 0, 0);

        m_empty = new QWidget(m_content);
        m_empty->setObjectName(QStringLiteral("emptyPanel%1").arg(panel));
        auto *emptyLayout = new QVBoxLayout(m_empty);
        emptyLayout->addStretch();
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

    void setPage(QWidget *page, const QString &title)
    {
        if (m_page == page) {
            setTitle(title);
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
        setTitle(title);
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

    void setTitle(const QString &title)
    {
        m_header->setText(title.isEmpty() ? tr("Terminal") : title);
    }

    void setActive(bool active)
    {
        QPalette palette = m_header->palette();
        palette.setColor(QPalette::Window,
                         palette.color(active ? QPalette::Highlight
                                              : QPalette::AlternateBase));
        palette.setColor(QPalette::WindowText,
                         palette.color(active ? QPalette::HighlightedText
                                              : QPalette::Text));
        m_header->setPalette(palette);
        setLineWidth(active ? 2 : 1);
    }

protected:
    bool eventFilter(QObject *watched, QEvent *event) override
    {
        if ((event->type() == QEvent::MouseButtonPress
             || event->type() == QEvent::FocusIn)
            && (watched == m_header || m_watched.contains(watched))) {
            m_owner->activateFromPanel(m_panel);
        }
        return QFrame::eventFilter(watched, event);
    }

private:
    void showEmpty()
    {
        m_stack->setCurrentWidget(m_empty);
        m_empty->show();
        m_header->setText(tr("New connection"));
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
    QLabel *m_header = nullptr;
    QWidget *m_content = nullptr;
    QStackedLayout *m_stack = nullptr;
    QWidget *m_empty = nullptr;
    QWidget *m_page = nullptr;
    QVector<QObject *> m_watched;
};

PanelContainer::PanelContainer(QWidget *parent)
    : QWidget(parent)
{
    auto *outer = new QVBoxLayout(this);
    outer->setContentsMargins(0, 0, 0, 0);
    outer->setSpacing(0);

    m_tabs = new QTabBar(this);
    m_tabs->setObjectName(QStringLiteral("connectionTabBar"));
    m_tabs->setAutoHide(true);
    m_tabs->setDocumentMode(true);
    m_tabs->setMovable(true);
    m_tabs->setExpanding(false);
    outer->addWidget(m_tabs);

    m_gridWidget = new QWidget(this);
    m_gridWidget->setObjectName(QStringLiteral("panelGrid"));
    auto *grid = new QGridLayout(m_gridWidget);
    grid->setContentsMargins(0, 0, 0, 0);
    grid->setSpacing(2);
    outer->addWidget(m_gridWidget, 1);

    for (int i = 0; i < 4; i++) {
        m_frames.append(new PaneFrame(this, i));
    }
    arrangeFrames();

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
        }
    });
}

namespace {
PaneFrame *frameAt(const QVector<PaneFrame *> &frames, int panel)
{
    if (panel < 0 || panel >= frames.size()) {
        return nullptr;
    }
    return frames[panel];
}
} // namespace

QWidget *PanelContainer::widget(int index) const
{
    return index >= 0 && index < m_pages.size() ? m_pages[index] : nullptr;
}

void PanelContainer::setCurrentIndex(int index) { setCurrentWidget(widget(index)); }

void PanelContainer::setCurrentWidget(QWidget *page)
{
    if (!page || !m_pages.contains(page)) {
        return;
    }

    bool assignmentChanged = false;
    if (panelOf(page) < 0) {
        int panel = panelOf(m_current);
        if (panel < 0) {
            panel = 0;
        }
        assign(panel, page);
        assignmentChanged = true;
    }

    const bool currentDidChange = m_current != page;
    m_current = page;
    {
        const QSignalBlocker block(m_tabs);
        m_tabs->setCurrentIndex(indexOf(page));
    }
    updateActiveFrames();
    if (assignmentChanged) {
        emit visiblePagesChanged();
    }
    if (currentDidChange) {
        emit currentChanged(page);
    }
}

int PanelContainer::addPage(QWidget *page, const QString &title,
                            int preferredPanel)
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

    int panel = -1;
    if (preferredPanel >= 0 && preferredPanel < panelCount()) {
        panel = preferredPanel;
    } else {
        panel = firstEmptyPanel();
        if (panel < 0) {
            panel = panelOf(m_current);
        }
        if (panel < 0) {
            panel = 0;
        }
    }
    assign(panel, page);
    setCurrentWidget(page);
    emit visiblePagesChanged();
    return index;
}

QWidget *PanelContainer::removePage(int index)
{
    QWidget *page = widget(index);
    if (!page) {
        return nullptr;
    }
    const int panel = panelOf(page);
    const bool wasCurrent = page == m_current;
    if (panel >= 0) {
        frameAt(m_frames, panel)->takePage();
    }

    m_changingTabs = true;
    m_tabs->removeTab(index);
    m_changingTabs = false;
    m_pages.removeAt(index);
    page->hide();
    page->setParent(this);

    QWidget *replacement = nullptr;
    if (panel >= 0) {
        replacement = firstHiddenPage();
        if (replacement) {
            assign(panel, replacement);
        }
    }
    if (wasCurrent) {
        m_current = nullptr;
        if (!replacement) {
            for (int i = 0; i < panelCount(); i++) {
                if ((replacement = pageAtPanel(i))) {
                    break;
                }
            }
        }
        if (replacement) {
            setCurrentWidget(replacement);
        }
    } else {
        updateActiveFrames();
        const QSignalBlocker block(m_tabs);
        m_tabs->setCurrentIndex(currentIndex());
    }
    emit visiblePagesChanged();
    return page;
}

void PanelContainer::setTabText(int index, const QString &title)
{
    if (!widget(index)) {
        return;
    }
    m_tabs->setTabText(index, title);
    const int panel = panelOf(widget(index));
    if (panel >= 0) {
        frameAt(m_frames, panel)->setTitle(title);
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

    QVector<QWidget *> order;
    const auto append = [&order](QWidget *page) {
        if (page && !order.contains(page)) {
            order.append(page);
        }
    };
    append(m_current);
    for (int i = 0; i < panelCount(); i++) {
        append(pageAtPanel(i));
    }
    for (QWidget *page : m_pages) {
        append(page);
    }

    for (int i = 0; i < m_frames.size(); i++) {
        frameAt(m_frames, i)->takePage();
    }
    m_layout = layout;
    arrangeFrames();
    for (int i = 0; i < panelCount() && i < order.size(); i++) {
        assign(i, order[i]);
    }
    updateActiveFrames();
    emit visiblePagesChanged();
}

QWidget *PanelContainer::pageAtPanel(int panel) const
{
    PaneFrame *frame = frameAt(m_frames, panel);
    return panel < panelCount() && frame ? frame->page() : nullptr;
}

int PanelContainer::panelOf(QWidget *page) const
{
    for (int i = 0; i < panelCount(); i++) {
        if (pageAtPanel(i) == page) {
            return i;
        }
    }
    return -1;
}

int PanelContainer::firstEmptyPanel() const
{
    for (int i = 0; i < panelCount(); i++) {
        if (!pageAtPanel(i)) {
            return i;
        }
    }
    return -1;
}

QVector<QWidget *> PanelContainer::visiblePages() const
{
    QVector<QWidget *> out;
    for (int i = 0; i < panelCount(); i++) {
        if (QWidget *page = pageAtPanel(i)) {
            out.append(page);
        }
    }
    return out;
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

void PanelContainer::assign(int panel, QWidget *page)
{
    PaneFrame *destination = frameAt(m_frames, panel);
    if (!destination || panel >= panelCount()) {
        return;
    }
    const int oldPanel = panelOf(page);
    if (oldPanel >= 0 && oldPanel != panel) {
        frameAt(m_frames, oldPanel)->takePage();
    }
    destination->setPage(page, m_tabs->tabText(indexOf(page)));
}

void PanelContainer::arrangeFrames()
{
    auto *grid = static_cast<QGridLayout *>(m_gridWidget->layout());
    for (int i = 0; i < m_frames.size(); i++) {
        PaneFrame *frame = frameAt(m_frames, i);
        grid->removeWidget(frame);
        frame->setVisible(i < panelCount());
    }

    if (m_layout == PanelLayout::Single) {
        grid->addWidget(frameAt(m_frames, 0), 0, 0);
    } else if (m_layout == PanelLayout::Two) {
        grid->addWidget(frameAt(m_frames, 0), 0, 0);
        grid->addWidget(frameAt(m_frames, 1), 0, 1);
    } else {
        for (int i = 0; i < 4; i++) {
            grid->addWidget(frameAt(m_frames, i), i / 2, i % 2);
        }
    }
    for (int row = 0; row < 2; row++) {
        grid->setRowStretch(row, row < (m_layout == PanelLayout::Four ? 2 : 1));
    }
    for (int col = 0; col < 2; col++) {
        const bool used = col == 0 || m_layout != PanelLayout::Single;
        grid->setColumnStretch(col, used ? 1 : 0);
    }
}

void PanelContainer::updateActiveFrames()
{
    for (int i = 0; i < m_frames.size(); i++) {
        frameAt(m_frames, i)->setActive(i < panelCount()
                                              && pageAtPanel(i) == m_current);
    }
}

QWidget *PanelContainer::firstHiddenPage() const
{
    for (QWidget *page : m_pages) {
        if (panelOf(page) < 0) {
            return page;
        }
    }
    return nullptr;
}
