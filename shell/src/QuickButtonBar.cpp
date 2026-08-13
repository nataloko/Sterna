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
        action->setToolTip(tip);
        action->setStatusTip(button.describe());
        connect(action, &QAction::triggered, this, [this, i] {
            // Read at the moment of the press rather than from the event: a
            // shortcut and a click arrive through different paths and this is
            // the one answer both of them have.
            const bool shift =
                QApplication::keyboardModifiers().testFlag(Qt::ShiftModifier);
            emit activated(i, shift);
        });
        m_actions.append(action);
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

void QuickButtonBar::refresh(const Session *session)
{
    const bool live = session && session->isConnected();
    for (int i = 0; i < m_actions.size(); i++) {
        // A menu command is the window's, not the wire's — Save setup and the
        // settings dialog work perfectly well with nothing connected, so
        // greying those out would be wrong for the sake of a rule.
        const bool needsLink = m_buttons[i].kind != TT_QUICK_BUTTON_COMMAND;
        m_actions[i]->setEnabled(live || !needsLink);
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
