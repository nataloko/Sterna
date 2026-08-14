// The bar of user-defined commands.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QToolBar>
#include <QVector>

#include "QuickButtons.h"

class QAction;
class Session;

/// A toolbar of the commands somebody keeps one click away.
///
/// Upstream has nothing like it — the nearest thing is a `KEYBOARD.CNF` user
/// key, which is the same four actions with no face on them. This is
/// deliberately a second toolbar rather than more items on `ConnectBar`: that
/// one is three things a serial console needs every few minutes and it is not
/// a general toolbar, which is the point of its own comment.
///
/// It owns no state either. The list belongs to the window, which read it out
/// of the settings file; the bar builds actions from it and reports a press.
/// It can be dragged to any of the four edges — on the left or the right it
/// costs no terminal rows, which on a short window is what makes the feature
/// worth having.
class QuickButtonBar : public QToolBar {
    Q_OBJECT

public:
    explicit QuickButtonBar(QWidget *parent = nullptr);

    /// Rebuild from `buttons`. The window calls this at startup and whenever
    /// the editor has been through.
    void setButtons(const QVector<QuickButton> &buttons);
    const QVector<QuickButton> &buttons() const { return m_buttons; }

    /// Enable or disable every button from the session, the way `ConnectBar`
    /// does: a command with nowhere to go is not a command.
    void refresh(const Session *session);

    /// Show `index` as repeating, with `remaining` sends to come — -1 for a
    /// run with no end, and 0 for one that has finished or was never started.
    ///
    /// The clock is `QuickButtonRepeat`'s and the list is the window's; this
    /// only paints the answer.
    void setRepeating(int index, int remaining);

signals:
    /// A button was pressed. `withoutEnter` is a Shift+click, which sends the
    /// command with its trailing Return left off so it can be edited on the
    /// far end before it runs.
    void activated(int index, bool withoutEnter);
    /// The **+** at the end, or Add from the context menu.
    void addRequested();
    /// Edit from the context menu, or a click on nothing when the bar is
    /// otherwise empty.
    void editRequested(int index);
    void removeRequested(int index);
    void duplicateRequested(int index);
    /// Stop from the context menu. Pressing the button again also stops it,
    /// but that arrives as `activated` — the window owns the rule that a
    /// second press is a stop, because it is the one that knows a run started.
    void stopRequested(int index);

private:
    void showContextMenu(const QPoint &pos);
    /// Which button the widget under `pos` belongs to, or -1.
    int indexAt(const QPoint &pos) const;
    /// Caption and tooltip for `index`, including whatever it is doing now.
    void describeAction(int index);

    QVector<QuickButton> m_buttons;
    QVector<QAction *> m_actions;
    /// Per button: sends left, -1 for a run with no end, 0 for not running.
    QVector<int> m_remaining;
    QAction *m_add = nullptr;
};
