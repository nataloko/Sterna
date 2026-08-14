// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "QuickButtonBar.h"

#include <QAction>
#include <QApplication>
#include <QMenu>

#include "Session.h"

QuickButtonBar::QuickButtonBar(QWidget *parent) : QToolBar(parent)
{
    setObjectName(QStringLiteral("quickButtonBar"));
    setWindowTitle(tr("Quick buttons"));
    // Movable, unlike `ConnectBar`. That bar has a fixed job in a fixed place;
    // this one is the user's own and how much of the window it may cost is
    // theirs to decide — an edge with no rows to spare is what the left and
    // right areas are for.
    setMovable(true);
    setFloatable(false);
    // Text: there is no icon theme this program ships, and there is certainly
    // no themed icon for "show version".
    setToolButtonStyle(Qt::ToolButtonTextOnly);

    setContextMenuPolicy(Qt::CustomContextMenu);
    connect(this, &QWidget::customContextMenuRequested, this,
            &QuickButtonBar::showContextMenu);
}

void QuickButtonBar::setButtons(const QVector<QuickButton> &buttons)
{
    m_buttons = buttons;
    m_actions.clear();
    // Nothing is repeating across a rebuild: the indices these count against
    // have just been renumbered, and the window stops every run for the same
    // reason.
    m_remaining.fill(0, buttons.size());
    m_add = nullptr;

    // **`QToolBar::clear()` removes its actions and does not delete them**, and
    // `addAction(text)` parents them here — so a rebuild without this leaves
    // every previous action alive as a child of the bar, holding its shortcut
    // and answering `findChild` before the live one does. The symptom is a
    // button that stops following the session.
    const QList<QAction *> previous = actions();
    clear();
    qDeleteAll(previous);

    for (int i = 0; i < m_buttons.size(); i++) {
        const QuickButton &button = m_buttons[i];
        QAction *action = addAction(button.shortCaption());
        action->setObjectName(QStringLiteral("quickButton%1").arg(i));
        // Checkable only for a button that can repeat, because "on" here means
        // a run in progress and a button that cannot start one must never look
        // as though it has.
        action->setCheckable(button.repeats());
        m_actions.append(action);
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
        addSeparator();
        m_add = addAction(QStringLiteral("+"));
        m_add->setObjectName(QStringLiteral("quickButtonAdd"));
        m_add->setToolTip(tr("Add a quick button..."));
        connect(m_add, &QAction::triggered, this,
                &QuickButtonBar::addRequested);
    }
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
        tip += QLatin1Char('\n') + tr("Asks before running.");
    }
    if (button.sendsEnter()) {
        tip += QLatin1Char('\n') + tr("Shift+click sends it without Enter.");
    }
    if (left < 0) {
        tip += QLatin1Char('\n') + tr("Repeating. Press again to stop.");
    } else if (left > 0) {
        tip += QLatin1Char('\n')
            + tr("Repeating: %n send(s) to go. Press again to stop.", nullptr,
                 left);
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
        for (int i = 0; i < m_actions.size(); i++) {
            if (widgetForAction(m_actions[i]) == child) {
                return i;
            }
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
