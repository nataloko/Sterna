// The bar of user-defined commands.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QVector>
#include <QWidget>

#include "QuickButtons.h"

class QAction;
class QBoxLayout;
class QFrame;
class QToolButton;
class Session;

/// A panel of the commands somebody keeps one click away.
///
/// Upstream has nothing like it — the nearest thing is a `KEYBOARD.CNF` user
/// key, which is the same four actions with no face on them. This is
/// deliberately its own panel rather than more items on `ConnectBar`: that one
/// is three things a serial console needs every few minutes and it is not a
/// general toolbar, which is the point of its own comment.
///
/// It owns no state either. The list belongs to the window, which read it out
/// of the settings file; the panel builds actions from it and reports a press.
/// Its enclosing dock can be dragged to any of the four edges and resized —
/// on the left or the right it costs no terminal rows, which on a short window
/// is what makes the feature worth having.
///
/// **A plain widget and a box layout rather than a `QToolBar`**, which is what
/// this was until the buttons had to fill the panel. `QToolBarLayout` sizes
/// every item to its own text and centres it across the bar's thickness, and
/// it does that whatever size policy the button carries — so a dragged-wider
/// dock put all of its new room in the margins around a ragged column of
/// captions. The one lever that does move it, a minimum width on each button,
/// raises the bar's own minimum with it and the splitter can then grow but
/// never shrink. Measured, both of them. A box layout gives each button the
/// panel's full width for free and needs no lever at all.
class QuickButtonBar : public QWidget {
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

    /// Stack the buttons down the panel or lay them across it. The window
    /// calls this from the dock's own edge, which the user can drag.
    void setOrientation(Qt::Orientation orientation);
    Qt::Orientation orientation() const { return m_orientation; }

    /// The widget `index` is pressed through, or null. The window has no use
    /// for it; a test measuring how wide a button ended up does.
    QToolButton *buttonWidget(int index) const;

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
    /// A button for `action`, added before the trailing stretch.
    QToolButton *addButton(QAction *action);
    /// Empty the layout and delete everything that was in it, actions
    /// included, leaving the stretch that holds what comes next to the top.
    void clearContents();

    QVector<QuickButton> m_buttons;
    /// Whether `setButtons` has laid the panel out once. Until it has, an
    /// unchanged list is still a rebuild worth doing — see `setButtons`.
    bool m_built = false;
    QVector<QAction *> m_actions;
    QVector<QToolButton *> m_widgets;
    /// Per button: sends left, -1 for a run with no end, 0 for not running.
    QVector<int> m_remaining;
    QAction *m_add = nullptr;
    QBoxLayout *m_layout = nullptr;
    QFrame *m_separator = nullptr;
    Qt::Orientation m_orientation = Qt::Vertical;
};
