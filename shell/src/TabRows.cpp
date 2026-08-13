// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "TabRows.h"

#include <QFontMetrics>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QStyle>
#include <QStyleOptionTab>
#include <QStylePainter>
#include <QTabBar>

TabRows::TabRows(QWidget *parent) : QWidget(parent)
{
    // `setHeightForWidth` is the load-bearing half: without it a layout never
    // asks [`heightForWidth`], and a bar that has wrapped onto three rows is
    // given the height of one.
    QSizePolicy policy(QSizePolicy::Preferred, QSizePolicy::Minimum);
    policy.setHeightForWidth(true);
    setSizePolicy(policy);
    // What a `QTabBar` uses: a click must not steal focus from the page's
    // editors, and Tab still reaches the bar.
    setFocusPolicy(Qt::TabFocus);
    setMouseTracking(true);
}

int TabRows::addTab(const QString &text)
{
    Tab tab;
    tab.text = text;
    m_tabs.append(tab);
    m_hintsDirty = true;
    const int index = count() - 1;
    relayout();
    updateGeometry();
    update();
    if (m_current < 0) {
        m_current = index;
        emit currentChanged(index);
    }
    return index;
}

QString TabRows::tabText(int index) const
{
    return index >= 0 && index < count() ? m_tabs.at(index).text : QString();
}

void TabRows::setCurrentIndex(int index)
{
    if (index < 0 || index >= count() || index == m_current) {
        return;
    }
    m_current = index;
    update();
    emit currentChanged(index);
}

QSize TabRows::tabHint(const QString &text) const
{
    QStyleOptionTab opt;
    opt.initFrom(this);
    opt.shape = QTabBar::RoundedNorth;
    opt.text = text;
    const int hframe = style()->pixelMetric(QStyle::PM_TabBarTabHSpace, &opt, this);
    const int vframe = style()->pixelMetric(QStyle::PM_TabBarTabVSpace, &opt, this);
    const QFontMetrics fm = fontMetrics();
    const QSize contents(fm.size(Qt::TextShowMnemonic, text).width() + hframe,
                         fm.height() + vframe);
    return style()->sizeFromContents(QStyle::CT_TabBarTab, &opt, contents, this);
}

void TabRows::ensureHints() const
{
    if (!m_hintsDirty) {
        return;
    }
    m_hintsDirty = false;
    for (Tab &tab : m_tabs) {
        tab.hint = tabHint(tab.text);
    }
}

int TabRows::layout(int width, QVector<Tab> *out) const
{
    ensureHints();
    QVector<Tab> tabs = m_tabs;
    if (tabs.isEmpty()) {
        if (out) {
            *out = tabs;
        }
        return 0;
    }

    const int avail = qMax(1, width);
    int y = 0;
    int row = 0;
    int first = 0;
    while (first < tabs.size()) {
        // At least one tab per row, however narrow the widget is: a row that
        // could hold nothing would not terminate.
        int last = first;
        int used = tabs.at(first).hint.width();
        while (last + 1 < tabs.size()
               && used + tabs.at(last + 1).hint.width() <= avail) {
            last++;
            used += tabs.at(last).hint.width();
        }

        // Justified to the full width, which is what a multiline tab control
        // does and what keeps the bar from ending in a ragged edge above the
        // page's frame. The remainder goes one pixel at a time to the left.
        const int n = last - first + 1;
        const int extra = avail > used ? avail - used : 0;
        int height = 0;
        int x = 0;
        for (int i = first; i <= last; i++) {
            const int share = extra / n + ((i - first) < extra % n ? 1 : 0);
            Tab &tab = tabs[i];
            tab.rect = QRect(x, y, tab.hint.width() + share, tab.hint.height());
            tab.row = row;
            tab.first = i == first;
            tab.last = i == last;
            x += tab.rect.width();
            height = qMax(height, tab.hint.height());
        }
        for (int i = first; i <= last; i++) {
            tabs[i].rect.setHeight(height);
        }

        y += height;
        row++;
        first = last + 1;
    }

    if (out) {
        *out = tabs;
    }
    return y;
}

void TabRows::relayout()
{
    const int rows = m_rows;
    layout(width(), &m_tabs);
    m_rows = m_tabs.isEmpty() ? 1 : m_tabs.constLast().row + 1;
    if (rows != m_rows) {
        // The height this widget wants has changed, so the layout has to ask
        // again. Without this the extra row is laid out and then clipped.
        updateGeometry();
    }
}

int TabRows::rowsForWidth(int width) const
{
    QVector<Tab> probe;
    layout(width, &probe);
    return probe.isEmpty() ? 1 : probe.constLast().row + 1;
}

int TabRows::widthForRows(int rows) const
{
    if (m_tabs.isEmpty() || rows < 1) {
        return 0;
    }
    ensureHints();
    int narrowest = 0;
    int widest = 0;
    for (const Tab &tab : m_tabs) {
        narrowest = qMax(narrowest, tab.hint.width());
        widest += tab.hint.width();
    }
    if (rows == 1) {
        return widest;
    }
    // Fewer rows never need less width, so this is a search for the boundary
    // rather than a scan.
    int lo = narrowest;
    int hi = widest;
    while (lo < hi) {
        const int mid = lo + (hi - lo) / 2;
        if (rowsForWidth(mid) <= rows) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    return lo;
}

QSize TabRows::minimumSizeHint() const
{
    // Every hint here is measured in the style's font, and a widget has the
    // application's until it is polished — which happens on the first show,
    // *after* a dialog has asked how big it wants to be. Asking for the polish
    // here makes the answer the same before and after, so the dialog cannot
    // open at a width measured in a font it is not using. `QComboBox::sizeHint`
    // does the same, for the same reason.
    ensurePolished();
    ensureHints();
    int narrowest = 0;
    for (const Tab &tab : m_tabs) {
        narrowest = qMax(narrowest, tab.hint.width());
    }
    // One tab wide is a real minimum width. The matching *height* is not a
    // useful minimum, and this is the trap in a wrapping widget: a layout
    // computes its minimum width and its minimum height independently, so
    // returning the 25-row height that one-tab width implies makes the whole
    // dialog 900 pixels tall at every width. The height belongs to
    // [`heightForWidth`], which is asked with the width the dialog really has;
    // the minimum quoted here is the two-row arrangement it opens in. A
    // word-wrapped `QLabel` makes the same bargain.
    return QSize(narrowest, heightForWidth(widthForRows(2)));
}

QSize TabRows::sizeHint() const
{
    // Two rows, deliberately, and not every tab in one row: that is a
    // `QTabBar`'s hint and it would ask a 25-page dialog to open two thousand
    // pixels wide. `minimumSizeHint` still allows one tab per row, so the
    // dialog opens at two and wraps rather than clips when it is made narrower.
    ensurePolished();
    const int w = widthForRows(2);
    return QSize(w, heightForWidth(w));
}

int TabRows::heightForWidth(int width) const
{
    return layout(width, nullptr);
}

void TabRows::resizeEvent(QResizeEvent *event)
{
    QWidget::resizeEvent(event);
    relayout();
}

void TabRows::changeEvent(QEvent *event)
{
    if (event->type() == QEvent::FontChange || event->type() == QEvent::StyleChange) {
        // Marked, not remeasured: see `ensureHints`.
        m_hintsDirty = true;
        updateGeometry();
        update();
    }
    QWidget::changeEvent(event);
}

void TabRows::paintEvent(QPaintEvent *)
{
    QStylePainter painter(this);
    for (int i = 0; i < count(); i++) {
        const Tab &tab = m_tabs.at(i);
        QStyleOptionTab opt;
        opt.initFrom(this);
        opt.rect = tab.rect;
        opt.shape = QTabBar::RoundedNorth;
        opt.text = tab.text;
        opt.row = tab.row;
        opt.position = tab.first && tab.last ? QStyleOptionTab::OnlyOneTab
                       : tab.first          ? QStyleOptionTab::Beginning
                       : tab.last           ? QStyleOptionTab::End
                                            : QStyleOptionTab::Middle;
        // Only within a row: the tab at the end of one row and the tab at the
        // start of the next are not neighbours on screen.
        opt.selectedPosition = QStyleOptionTab::NotAdjacent;
        if (m_current >= 0 && m_current < count()
            && m_tabs.at(m_current).row == tab.row) {
            if (m_current == i + 1) {
                opt.selectedPosition = QStyleOptionTab::NextIsSelected;
            } else if (m_current == i - 1) {
                opt.selectedPosition = QStyleOptionTab::PreviousIsSelected;
            }
        }
        opt.state.setFlag(QStyle::State_Selected, i == m_current);
        opt.state.setFlag(QStyle::State_MouseOver, i == m_hover);
        opt.state.setFlag(QStyle::State_HasFocus, i == m_current && hasFocus());
        painter.drawControl(QStyle::CE_TabBarTab, opt);
    }
}

int TabRows::tabAt(const QPoint &pos) const
{
    for (int i = 0; i < count(); i++) {
        if (m_tabs.at(i).rect.contains(pos)) {
            return i;
        }
    }
    return -1;
}

void TabRows::mousePressEvent(QMouseEvent *event)
{
    if (event->button() != Qt::LeftButton) {
        QWidget::mousePressEvent(event);
        return;
    }
    const int index = tabAt(event->position().toPoint());
    if (index >= 0) {
        setCurrentIndex(index);
    }
}

void TabRows::mouseMoveEvent(QMouseEvent *event)
{
    const int index = tabAt(event->position().toPoint());
    if (index != m_hover) {
        m_hover = index;
        update();
    }
    QWidget::mouseMoveEvent(event);
}

void TabRows::leaveEvent(QEvent *event)
{
    if (m_hover != -1) {
        m_hover = -1;
        update();
    }
    QWidget::leaveEvent(event);
}

void TabRows::keyPressEvent(QKeyEvent *event)
{
    if (m_current < 0) {
        QWidget::keyPressEvent(event);
        return;
    }
    switch (event->key()) {
    case Qt::Key_Left:
        setCurrentIndex(m_current - 1);
        return;
    case Qt::Key_Right:
        setCurrentIndex(m_current + 1);
        return;
    case Qt::Key_Up:
    case Qt::Key_Down: {
        // The tab nearest the current one horizontally in the row above or
        // below, so arrowing up and down walks a column rather than jumping to
        // the start of a row.
        const int wanted =
            m_tabs.at(m_current).row + (event->key() == Qt::Key_Up ? -1 : 1);
        const int centre = m_tabs.at(m_current).rect.center().x();
        int best = -1;
        int distance = 0;
        for (int i = 0; i < count(); i++) {
            if (m_tabs.at(i).row != wanted) {
                continue;
            }
            const int d = qAbs(m_tabs.at(i).rect.center().x() - centre);
            if (best < 0 || d < distance) {
                best = i;
                distance = d;
            }
        }
        if (best >= 0) {
            setCurrentIndex(best);
        }
        return;
    }
    default:
        break;
    }
    QWidget::keyPressEvent(event);
}
