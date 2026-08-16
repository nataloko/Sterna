// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "QuickButtonBar.h"

#include <QAction>
#include <QActionGroup>
#include <QApplication>
#include <QBoxLayout>
#include <QComboBox>
#include <QFrame>
#include <QMenu>
#include <QSignalBlocker>
#include <QSizePolicy>
#include <QHelpEvent>
#include <QStyleOptionComboBox>
#include <QStyleOptionToolButton>
#include <QStylePainter>
#include <QToolButton>
#include <QToolTip>

#include "Session.h"

namespace {

/// A tool button that shortens its caption instead of demanding room for it.
///
/// **`QToolButton` will not go narrower than its own text**, and it enforces
/// that through `minimumSizeHint` — which is the panel's minimum too, so the
/// longest caption on the bar decided how narrow the whole panel could be. A
/// person who wants a strip of stubs down the edge of the screen, or who put
/// a sentence on one button and does not want the other nine widened for it,
/// could not have one.
///
/// So the minimum is dropped and the caption is elided at paint time. The full
/// text stays in `text()`, so the tooltip, the context menu and every test
/// asking what a button says get the real answer — only the pixels are short.
///
/// Defined here rather than in the header for the same reason `PaneFrame` is:
/// nothing outside this file constructs one.
class BarButton : public QToolButton {
public:
    using QToolButton::QToolButton;

    QSize minimumSizeHint() const override
    {
        // The height a button needs, and no width demand at all. The floor
        // that stops the panel becoming a sliver is the window's
        // (`kQuickPanelMinWidth`), where it can be one number rather than a
        // consequence of whatever somebody last typed into a caption.
        QSize hint = QToolButton::minimumSizeHint();
        hint.setWidth(0);
        return hint;
    }

    /// The caption, when the pixels no longer carry it.
    ///
    /// A tooltip here describes what the button *sends*, which is the useful
    /// half while the label is legible and not enough on its own once it is
    /// three letters and an ellipsis — two buttons on a 48-pixel panel can
    /// both read `Sh…`. So the full caption joins the tooltip exactly when the
    /// paint had to shorten it, and stays out of the way the rest of the time.
    bool event(QEvent *event) override
    {
        if (event->type() == QEvent::ToolTip && m_elided && !text().isEmpty()) {
            const QString tip = toolTip().isEmpty()
                ? text()
                : text() + QLatin1Char('\n') + toolTip();
            QToolTip::showText(static_cast<QHelpEvent *>(event)->globalPos(),
                               tip, this);
            return true;
        }
        return QToolButton::event(event);
    }

protected:
    void paintEvent(QPaintEvent *) override
    {
        QStylePainter painter(this);
        QStyleOptionToolButton option;
        initStyleOption(&option);
        // The room the style will actually put text in. Asked of the style
        // rather than assumed, then inset by the same margin the style uses,
        // because a button drawn to its own frame reads as clipped rather
        // than as shortened.
        const QRect area =
            style()->subControlRect(QStyle::CC_ToolButton, &option,
                                    QStyle::SC_ToolButton, this);
        const int margin =
            2 * style()->pixelMetric(QStyle::PM_ButtonMargin, &option, this);
        const int room = area.width() - margin;
        if (room > 0) {
            option.text = option.fontMetrics.elidedText(option.text,
                                                        Qt::ElideRight, room);
        }
        m_elided = option.text != text();
        painter.drawComplexControl(QStyle::CC_ToolButton, option);
    }

private:
    /// Whether the last paint had to shorten the caption. Read by `event`,
    /// which is the only thing that needs to know.
    bool m_elided = false;
};

/// The page drop-down, which shortens its text for `BarButton`'s reason.
///
/// `QComboBox::minimumSizeHint` sizes to its **longest item** plus the arrow,
/// so one page called `Out-of-band management` would hold the whole panel open
/// at a width nothing else on it asked for — undoing the work above. Same
/// answer: drop the minimum, elide at paint, leave the model alone so the
/// popup, the tooltip and every test still see the real name.
///
/// `sizeHint` is deliberately *not* touched. That one is what Panel width >
/// Fit to buttons measures, and a fit that cut the page name off would be a
/// panel nobody could read the top of.
class PageBox : public QComboBox {
public:
    using QComboBox::QComboBox;

    QSize minimumSizeHint() const override
    {
        QSize hint = QComboBox::minimumSizeHint();
        hint.setWidth(0);
        return hint;
    }

    bool event(QEvent *event) override
    {
        if (event->type() == QEvent::ToolTip && m_elided && !currentText().isEmpty()) {
            QToolTip::showText(static_cast<QHelpEvent *>(event)->globalPos(),
                               currentText(), this);
            return true;
        }
        return QComboBox::event(event);
    }

protected:
    void paintEvent(QPaintEvent *) override
    {
        QStylePainter painter(this);
        QStyleOptionComboBox option;
        initStyleOption(&option);
        const QRect area =
            style()->subControlRect(QStyle::CC_ComboBox, &option,
                                    QStyle::SC_ComboBoxEditField, this);
        if (area.width() > 0) {
            option.currentText = option.fontMetrics.elidedText(
                option.currentText, Qt::ElideRight, area.width());
        }
        m_elided = option.currentText != currentText();
        painter.drawComplexControl(QStyle::CC_ComboBox, option);
        painter.drawControl(QStyle::CE_ComboBoxLabel, option);
    }

private:
    bool m_elided = false;
};

} // namespace

QuickButtonBar::QuickButtonBar(QWidget *parent) : QWidget(parent)
{
    setObjectName(QStringLiteral("quickButtonBar"));
    setWindowTitle(tr("Quick buttons"));
    setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);

    m_layout = new QBoxLayout(QBoxLayout::TopToBottom, this);
    m_layout->setContentsMargins(4, 4, 4, 4);
    m_layout->setSpacing(4);
    // One trailing stretch, so the buttons keep the top of the panel and the
    // room the user has dragged out collects below them. Not centred along
    // this axis, deliberately: centring would move every button when the
    // window is resized or a button is added, and a bar of things to click
    // must not move — the same rule `describeAction` keeps a counter out of a
    // caption for. Everything added goes in before this item, which is the
    // only one `clearContents` leaves behind.
    m_layout->addStretch();

    setContextMenuPolicy(Qt::CustomContextMenu);
    connect(this, &QWidget::customContextMenuRequested, this,
            &QuickButtonBar::showContextMenu);
}

QToolButton *QuickButtonBar::addButton(QAction *action)
{
    auto *button = new BarButton(this);
    button->setDefaultAction(action);
    // Text: there is no icon theme this program ships, and there is certainly
    // no themed icon for "show version".
    button->setToolButtonStyle(Qt::ToolButtonTextOnly);
    // A framed button rather than a toolbar's flat one. A flat button shows
    // its extent only under the pointer, which on a panel-wide button reads as
    // a centred caption and nothing to press.
    button->setAutoRaise(false);
    // The terminal keeps the keyboard. A button that took focus would leave
    // the next keystroke going nowhere, and there is nothing to type here.
    button->setFocusPolicy(Qt::NoFocus);
    // Preferred in both directions: the layout hands an item the panel's full
    // width (or height, laid across) and the surplus along its own direction
    // goes to the trailing stretch, which is the only expanding item here.
    button->setSizePolicy(QSizePolicy::Preferred, QSizePolicy::Preferred);
    m_layout->insertWidget(m_layout->count() - 1, button);
    return button;
}

void QuickButtonBar::clearContents()
{
    // Take everything out and delete it, the stretch included, then put the
    // stretch back — a stretch is a spacer item with no widget, so this is
    // cheaper than finding the widgets among them.
    while (QLayoutItem *item = m_layout->takeAt(0)) {
        delete item->widget();
        delete item;
    }
    m_layout->addStretch();
    m_widgets.clear();
    m_separator = nullptr;
    // Deleted by the sweep above; the pointers have to follow them.
    m_pageBox = nullptr;
    m_addWidget = nullptr;

    // The actions are children of this widget rather than of the buttons, so
    // that `findChild` can install a shortcut on one; deleting the buttons
    // therefore does not take them with it. Left alive they would keep
    // answering their old shortcuts and hand `findChild` a button that is no
    // longer on screen — the symptom is a button that stops following the
    // session. `buttons_test` found it in the toolbar this replaced.
    qDeleteAll(m_actions);
    m_actions.clear();
    delete m_add;
    m_add = nullptr;
}

bool QuickButtonBar::wouldRebuild(const QuickButtonSet &set) const
{
    return !m_built || set != m_set;
}

void QuickButtonBar::setButtons(const QuickButtonSet &set)
{
    // **The early return is load bearing, and not for the widgets it saves.**
    // This runs on every settings change, and a rebuild throws away every
    // button and makes new ones — so the panel's size hint drops to its empty
    // width and comes back. It used to be the terminal that paid for that: the
    // dock holding this bar took those pixels off the central widget and gave
    // them back, and a few pixels either way is a column, which is a real
    // `Grid::resize`. Toggling line edit blanked the screen this way.
    //
    // The panel takes its pixels from the window now, so the flicker is a
    // window that jumps sideways rather than a terminal that loses text —
    // better, and still not something to do on every unrelated setting. The
    // other half never moved: a rebuild destroys every `QAction`, and the
    // shortcuts and the repeat state hang off those.
    //
    // `m_built` and not an empty check: the panel with no buttons on it still
    // has contents — the `+` that defines the first one is made here — so the
    // opening call has to run even though it changes nothing about the list.
    if (!wouldRebuild(set)) {
        return;
    }
    m_built = true;
    m_set = set;
    // Nothing is repeating across a rebuild: the indices these count against
    // have just been renumbered, and the window stops every run for the same
    // reason.
    m_remaining.fill(0, m_set.buttons.size());
    // A page whose last button has gone, and which has no name of its own, has
    // stopped existing; the panel must not be left pointing at it.
    m_page = qBound(1, m_page, pageCount());
    clearContents();

    // **Every button on every page gets an action**, and the object names are
    // positions in the whole list. A page filters what is *drawn*; the window's
    // shortcut loop, `QuickButtonRepeat`'s indices and every test's
    // `quickButton%1` lookup all speak the flat index and go on doing so.
    for (int i = 0; i < m_set.buttons.size(); i++) {
        const QuickButton &button = m_set.buttons[i];
        auto *action = new QAction(button.shortCaption(), this);
        action->setObjectName(QStringLiteral("quickButton%1").arg(i));
        // Checkable only for a button that can repeat, because "on" here means
        // a run in progress and a button that cannot start one must never look
        // as though it has.
        action->setCheckable(button.repeats());
        // **On the bar, whatever page it is on.** A shortcut is a key the host
        // stops receiving, and one that came and went with the page showing
        // would be a key whose meaning depends on a drop-down nobody looked
        // at. Qt registers one shortcut per action however many widgets it is
        // associated with, so the `BarButton` below adding it again is not an
        // ambiguity — that is two different actions holding one sequence.
        // Hiding the panel still hands every key back: these hang off a widget
        // inside it.
        addAction(action);
        m_actions.append(action);
        connect(action, &QAction::triggered, this, [this, i, action] {
            // Qt has already toggled a checkable action by the time this runs,
            // and whether a run is on is not the press's to decide — a
            // confirmation may yet be declined. Put it back; the window sets
            // the true state through `setRepeating` a moment later.
            action->setChecked(m_remaining.value(i) != 0);
            // Read at the moment of the press rather than from the event: a
            // shortcut and a click arrive through different paths and this is
            // the one answer both of them have.
            const bool shift =
                QApplication::keyboardModifiers().testFlag(Qt::ShiftModifier);
            emit activated(i, shift);
        });
    }

    // Only when there is a second page, the same rule that keeps the bar itself
    // out of the way until a button exists — so a panel nobody has made a page
    // on is the panel it always was, fitted width included.
    if (pageCount() > 1) {
        m_pageBox = new PageBox(this);
        m_pageBox->setObjectName(QStringLiteral("quickButtonPageBox"));
        // The terminal keeps the keyboard; there is nothing to type here.
        m_pageBox->setFocusPolicy(Qt::NoFocus);
        m_pageBox->setToolTip(tr("This list selects the page of buttons to show."));
        for (int p = 1; p <= pageCount(); p++) {
            m_pageBox->addItem(m_set.pageLabel(p));
        }
        // **Point it at the page actually showing, before anything is
        // connected.** A rebuild destroys the old box and the new one starts at
        // row 0, so a panel on page 2 — every editor OK, Move to page and
        // Remove goes through here — came back drawing page 2 with a drop-down
        // reading `Page 1`. `MainWindow` cannot repair it either: its
        // `setPage(askedPage)` early-returns on an unchanged page, so the box
        // is never told. Set before `connect` rather than under a blocker, so
        // there is no signal to suppress.
        m_pageBox->setCurrentIndex(m_page - 1);
        m_layout->insertWidget(0, m_pageBox);
        connect(m_pageBox, &QComboBox::currentIndexChanged, this,
                [this](int index) {
                    // **A combo popup opens under the pointer**, so the release
                    // that opened it names the row already showing. `setPage`
                    // returning early on an unchanged page is what absorbs
                    // that; it is load-bearing for correctness, not for speed.
                    const int before = m_page;
                    setPage(index + 1);
                    if (m_page != before) {
                        emit pageChanged(m_page);
                    }
                });
    }

    rebuildPageColumn();
}

void QuickButtonBar::rebuildPageColumn()
{
    // Take out only what belongs to the page: the buttons, the rule and the
    // `+`. The actions stay — they are the whole list's, and deleting them
    // would take every shortcut and every running repeat with them.
    for (QToolButton *widget : m_widgets) {
        delete widget;
    }
    // `fill` and not `assign`: the latter is Qt 6.6 and CI builds against the
    // Ubuntu container's 6.4.2, where this is the difference between a green
    // run and a compile error nothing local would have shown.
    m_widgets.fill(nullptr, m_set.buttons.size());
    delete m_separator;
    m_separator = nullptr;
    // **The action and its widget, both.** `m_addWidget` is not in `m_widgets`
    // — that vector is one slot per button — so deleting only the action left a
    // live `QToolButton` in the layout still reading `+`, and every page switch
    // added another.
    delete m_add;
    m_add = nullptr;
    delete m_addWidget;
    m_addWidget = nullptr;

    for (int i = 0; i < m_set.buttons.size(); i++) {
        if (static_cast<int>(m_set.buttons[i].page) != m_page) {
            continue;
        }
        m_widgets[i] = addButton(m_actions[i]);
        describeAction(i);
    }

    if (!m_set.buttons.isEmpty()) {
        m_separator = new QFrame(this);
        m_separator->setFrameShadow(QFrame::Sunken);
        // A rule across the panel, because the buttons run down it. There is
        // no other case: the panel is fixed to the right-hand side.
        m_separator->setFrameShape(QFrame::HLine);
        m_layout->insertWidget(m_layout->count() - 1, m_separator);
    }
    // The empty panel is still useful: this is its shortest route to the first
    // button, and it keeps View > Show quick buttons truthful before one has
    // been defined.
    m_add = new QAction(QStringLiteral("+"), this);
    m_add->setObjectName(QStringLiteral("quickButtonAdd"));
    m_add->setToolTip(tr("This button opens the editor for a new quick button."));
    m_addWidget = addButton(m_add);
    connect(m_add, &QAction::triggered, this, &QuickButtonBar::addRequested);
}

void QuickButtonBar::setPage(int page)
{
    const int wanted = qBound(1, page, pageCount());
    if (wanted == m_page && m_built) {
        return;
    }
    m_page = wanted;
    if (m_built) {
        rebuildPageColumn();
    }
    if (m_pageBox) {
        // Blocked: this is the programmatic half, and the drop-down's own
        // change is what reports through `pageChanged`.
        const QSignalBlocker block(m_pageBox);
        m_pageBox->setCurrentIndex(m_page - 1);
    }
}

QToolButton *QuickButtonBar::buttonWidget(int index) const
{
    return m_widgets.value(index, nullptr);
}

void QuickButtonBar::describeAction(int index)
{
    if (index < 0 || index >= m_actions.size()) {
        return;
    }
    const QuickButton &button = m_set.buttons[index];
    QAction *action = m_actions[index];
    const int left = m_remaining.value(index);

    // A fixed mark rather than the count, because the caption sets the
    // button's width: a number ticking down from 10 to 9 to 8 would shuffle
    // every button after this one along the bar between clicks, which is the
    // one thing a bar of things to click must not do. The count is in the
    // tooltip, where it costs nothing to change.
    action->setText(left != 0 ? tr("%1 ⟳").arg(button.shortCaption())
                              : button.shortCaption());
    action->setChecked(left != 0);

    // The payload, because a label is short by design and "Reload" on a
    // router is worth being sure about before pressing.
    QString tip = button.describe();
    if (!button.shortcut.isEmpty()) {
        tip += QStringLiteral(" (%1)").arg(button.shortcut);
    }
    if (button.confirm) {
        tip += QLatin1Char('\n')
            + tr("This button shows a confirmation dialog before it runs.");
    }
    if (button.sendsEnter()) {
        tip += QLatin1Char('\n')
            + tr("A Shift-click on this button sends the content without Enter.");
    }
    if (left < 0) {
        tip += QLatin1Char('\n')
            + tr("This button sends again and again. A second press stops the repeat.");
    } else if (left > 0) {
        tip += QLatin1Char('\n');
        tip += left == 1
                   ? tr("This button will send one more time. A second press stops the repeat.")
                   : tr("This button will send %1 more times. A second press stops the repeat.")
                         .arg(left);
    }
    action->setToolTip(tip);
    // One line: a status bar shows a line feed as a box, and the repeat's own
    // half of `describe` has one in it.
    QString status = button.describe();
    action->setStatusTip(status.replace(QLatin1Char('\n'), QLatin1Char(' ')));
}

void QuickButtonBar::setRepeating(int index, int remaining)
{
    if (index < 0 || index >= m_remaining.size()) {
        return;
    }
    if (m_remaining[index] == remaining) {
        return;
    }
    m_remaining[index] = remaining;
    describeAction(index);
}

void QuickButtonBar::refresh(const Session *session)
{
    const bool live = session && session->isConnected();
    for (int i = 0; i < m_actions.size(); i++) {
        // Only the two sending kinds need a wire. A macro may establish its
        // own connection, and a menu command such as Save setup works offline.
        const bool needsLink = m_set.buttons[i].kind == TT_QUICK_BUTTON_TEXT
            || m_set.buttons[i].kind == TT_QUICK_BUTTON_BYTES;
        // ...and a button in the middle of a run stays pressable whatever the
        // session says, because pressing it is how it is stopped.
        m_actions[i]->setEnabled(live || !needsLink
                                 || m_remaining.value(i) != 0);
    }
}

int QuickButtonBar::indexAt(const QPoint &pos) const
{
    // Up from whatever was actually under the cursor: a tool button's label
    // can be a child of its own, and the answer wanted is the action.
    for (QWidget *child = childAt(pos); child && child != this;
         child = child->parentWidget()) {
        // **The cast has to be checked now that `m_widgets` holds nulls.** A
        // right-click on the drop-down, the rule or the panel's own background
        // casts to null, and `indexOf(nullptr)` answers with the first button
        // that is not on this page — a context menu offering to remove
        // somebody else's command.
        if (auto *button = qobject_cast<QToolButton *>(child)) {
            const int index = m_widgets.indexOf(button);
            if (index >= 0) {
                return index;
            }
        }
    }
    return -1;
}

void QuickButtonBar::showContextMenu(const QPoint &pos)
{
    QMenu *menu = buildContextMenu(indexAt(pos));
    menu->exec(mapToGlobal(pos));
    delete menu;
}

/// The menu, built but not shown.
///
/// Separate from `showContextMenu` because `QMenu::exec` spins its own event
/// loop and a test cannot click through one — and this menu is now the
/// reachable route to the panel's width, so "the item is there and wired" is
/// worth being able to ask.
QMenu *QuickButtonBar::buildContextMenu(int index)
{
    auto *out = new QMenu(this);
    QMenu &menu = *out;
    if (index >= 0 && m_remaining.value(index) != 0) {
        // First, and above a separator: while something is repeating it is the
        // only thing anybody opened this menu for.
        QAction *stop = menu.addAction(tr("Stop repeating"));
        menu.addSeparator();
        connect(stop, &QAction::triggered, this,
                [this, index] { emit stopRequested(index); });
    }
    if (index >= 0) {
        QAction *edit = menu.addAction(tr("Edit..."));
        QAction *duplicate = menu.addAction(tr("Duplicate"));
        QAction *remove = menu.addAction(tr("Remove"));
        menu.addSeparator();
        connect(edit, &QAction::triggered, this,
                [this, index] { emit editRequested(index); });
        connect(duplicate, &QAction::triggered, this,
                [this, index] { emit duplicateRequested(index); });
        connect(remove, &QAction::triggered, this,
                [this, index] { emit removeRequested(index); });
    }
    // Where a button goes, on the button's own menu — the page it belongs to
    // is a property of the button and this is the shortest route to it.
    if (index >= 0 && pageCount() > 1) {
        QMenu *move = menu.addMenu(tr("Move to page"));
        move->setObjectName(QStringLiteral("quickMenuMoveToPage"));
        for (int p = 1; p <= pageCount(); p++) {
            if (p == static_cast<int>(m_set.buttons[index].page)) {
                continue;
            }
            QAction *to = move->addAction(m_set.pageLabel(p));
            to->setObjectName(QStringLiteral("quickMenuMoveToPage%1").arg(p));
            connect(to, &QAction::triggered, this,
                    [this, index, p] { emit moveToPageRequested(index, p); });
        }
        menu.addSeparator();
    }

    QAction *add = menu.addAction(tr("Add..."));
    add->setObjectName(QStringLiteral("quickMenuAdd"));
    connect(add, &QAction::triggered, this, &QuickButtonBar::addRequested);

    // The pages, for the same reason the width is here: this is where the hand
    // already is, and a drop-down at the top of the panel is a way to *choose*
    // a page rather than a way to make one.
    menu.addSeparator();
    QMenu *pages = menu.addMenu(tr("Page"));
    pages->setObjectName(QStringLiteral("quickMenuPage"));
    auto *group = new QActionGroup(pages);
    for (int p = 1; p <= pageCount(); p++) {
        QAction *show = pages->addAction(m_set.pageLabel(p));
        show->setObjectName(QStringLiteral("quickMenuPage%1").arg(p));
        show->setCheckable(true);
        show->setChecked(p == m_page);
        group->addAction(show);
        connect(show, &QAction::triggered, this, [this, p] {
            const int before = m_page;
            setPage(p);
            if (m_page != before) {
                emit pageChanged(m_page);
            }
        });
    }
    pages->addSeparator();
    QAction *edit = pages->addAction(tr("Add, rename or remove pages..."));
    edit->setObjectName(QStringLiteral("quickMenuEditPages"));
    connect(edit, &QAction::triggered, this, &QuickButtonBar::editPagesRequested);

    // **The width lives here because this is where the hand already is.** It is
    // an ordinary setting on Setup's Window page and reachable there too, but a
    // panel that looks draggable and is not needs the answer within reach of
    // the thing it is about — twenty-six pages of a settings dialog is not
    // within reach. Same argument the Add item makes one line above.
    menu.addSeparator();
    QMenu *width = menu.addMenu(tr("Panel width"));
    width->setObjectName(QStringLiteral("quickMenuWidth"));
    QAction *fit = width->addAction(tr("Fit to buttons"));
    fit->setObjectName(QStringLiteral("quickMenuFit"));
    fit->setCheckable(true);
    fit->setChecked(m_fitted);
    QAction *exact = width->addAction(tr("Set width..."));
    exact->setObjectName(QStringLiteral("quickMenuSetWidth"));
    connect(fit, &QAction::triggered, this, &QuickButtonBar::fitWidthRequested);
    connect(exact, &QAction::triggered, this,
            &QuickButtonBar::setWidthRequested);
    return out;
}
