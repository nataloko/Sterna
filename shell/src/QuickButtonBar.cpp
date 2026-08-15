// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "QuickButtonBar.h"

#include <QAction>
#include <QApplication>
#include <QBoxLayout>
#include <QFrame>
#include <QMenu>
#include <QSizePolicy>
#include <QToolButton>

#include "Session.h"

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
    auto *button = new QToolButton(this);
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

void QuickButtonBar::setButtons(const QVector<QuickButton> &buttons)
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
    if (m_built && buttons == m_buttons) {
        return;
    }
    m_built = true;
    m_buttons = buttons;
    // Nothing is repeating across a rebuild: the indices these count against
    // have just been renumbered, and the window stops every run for the same
    // reason.
    m_remaining.fill(0, buttons.size());
    clearContents();

    for (int i = 0; i < m_buttons.size(); i++) {
        const QuickButton &button = m_buttons[i];
        auto *action = new QAction(button.shortCaption(), this);
        action->setObjectName(QStringLiteral("quickButton%1").arg(i));
        // Checkable only for a button that can repeat, because "on" here means
        // a run in progress and a button that cannot start one must never look
        // as though it has.
        action->setCheckable(button.repeats());
        m_actions.append(action);
        m_widgets.append(addButton(action));
        describeAction(i);
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

    if (!m_buttons.isEmpty()) {
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
    addButton(m_add);
    connect(m_add, &QAction::triggered, this, &QuickButtonBar::addRequested);
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
    const QuickButton &button = m_buttons[index];
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
        const bool needsLink = m_buttons[i].kind == TT_QUICK_BUTTON_TEXT
            || m_buttons[i].kind == TT_QUICK_BUTTON_BYTES;
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
        const int index = m_widgets.indexOf(qobject_cast<QToolButton *>(child));
        if (index >= 0) {
            return index;
        }
    }
    return -1;
}

void QuickButtonBar::showContextMenu(const QPoint &pos)
{
    const int index = indexAt(pos);
    QMenu menu(this);
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
    QAction *add = menu.addAction(tr("Add..."));
    connect(add, &QAction::triggered, this, &QuickButtonBar::addRequested);
    menu.exec(mapToGlobal(pos));
}
