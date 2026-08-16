// The bar of user-defined commands.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QVector>
#include <QWidget>

#include "QuickButtons.h"

class QAction;
class QBoxLayout;
class QComboBox;
class QMenu;
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
/// It is fixed down the right-hand side, where it costs no terminal rows —
/// which on a short window is what makes the feature worth having — and as
/// wide as `window.quick_buttons_width` says, which the window applies by
/// moving its own outer edge rather than the terminal's. There is no handle to
/// drag; `docs/deviations.md` entry 7 says why one was built and removed.
///
/// **A plain widget and a box layout rather than a `QToolBar`**, which is what
/// this was until the buttons had to fill the panel. `QToolBarLayout` sizes
/// every item to its own text and centres it across the bar's thickness, and
/// it does that whatever size policy the button carries — so a dragged-wider
/// panel put all of its new room in the margins around a ragged column of
/// captions. The one lever that does move it, a minimum width on each button,
/// raises the bar's own minimum with it and the panel can then grow but never
/// shrink. Measured, both of them. A box layout gives each button the panel's
/// full width for free and needs no lever at all.
class QuickButtonBar : public QWidget {
    Q_OBJECT

public:
    explicit QuickButtonBar(QWidget *parent = nullptr);

    /// Rebuild from `set`. The window calls this at startup and whenever the
    /// editor has been through.
    void setButtons(const QuickButtonSet &set);
    const QVector<QuickButton> &buttons() const { return m_set.buttons; }
    const QuickButtonSet &set() const { return m_set; }

    /// Whether `setButtons` would rebuild — the same comparison it makes.
    ///
    /// The window asks because a rebuild renumbers, and a renumber is what has
    /// to stop every repeat. Asking the question twice in two expressions is
    /// how the two come apart: a rebuild without a stop leaves
    /// `QuickButtonRepeat` firing at indices that have moved.
    bool wouldRebuild(const QuickButtonSet &set) const;

    /// Which page is showing, counting from 1.
    int page() const { return m_page; }
    int pageCount() const { return m_set.pageCount(); }

    /// Show `page`, clamped to the pages that exist.
    ///
    /// Only the buttons are rebuilt: the actions, their shortcuts and the
    /// repeat state all belong to the whole list and survive. Silent — the
    /// drop-down is moved with its signals blocked, and nothing is emitted;
    /// `pageChanged` is what the drop-down itself reports through.
    void setPage(int page);

    /// Enable or disable every button from the session, the way `ConnectBar`
    /// does: a command with nowhere to go is not a command.
    void refresh(const Session *session);

    /// Show `index` as repeating, with `remaining` sends to come — -1 for a
    /// run with no end, and 0 for one that has finished or was never started.
    ///
    /// The clock is `QuickButtonRepeat`'s and the list is the window's; this
    /// only paints the answer.
    void setRepeating(int index, int remaining);

    /// The widget `index` is pressed through, or null. The window has no use
    /// for it; a test measuring how wide a button ended up does.
    QToolButton *buttonWidget(int index) const;

    /// The context menu for the button at `index`, or for the panel itself
    /// when `index` is -1. Owned by the caller.
    ///
    /// Public so a test can inspect it: `showContextMenu` execs it, and a
    /// modal loop is not something a test can click through.
    QMenu *buildContextMenu(int index);

    /// Whether the panel is measuring its own buttons rather than holding a
    /// width somebody chose — `window.quick_buttons_width` being 0.
    ///
    /// Display state, the way `setRepeating` is: it decides one tick in the
    /// context menu and nothing else. The bar has no session to ask, and the
    /// window that does calls this whenever the setting moves.
    void setFitted(bool fitted) { m_fitted = fitted; }

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
    /// Panel width > Fit to buttons — go back to measuring the captions.
    void fitWidthRequested();
    /// Panel width > Set width… — ask for a number. The window owns the
    /// prompt: it is the one that knows what the width will be clamped to and
    /// what the screen has room for.
    void setWidthRequested();
    /// The page showing has changed, from the drop-down or from the menu. The
    /// window writes it down; the panel has already moved.
    void pageChanged(int page);
    /// Move a button to another page — the button's own context menu.
    void moveToPageRequested(int index, int page);
    /// Add page… / Rename page… / Remove page, which the editor owns because
    /// it is the one holding a copy to edit.
    void editPagesRequested();

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
    /// Rebuild only what belongs to the page showing: the buttons, the rule
    /// under them and the **+**. The actions and the repeat state are the
    /// whole list's and are left alone, which is what lets a page switch keep
    /// a run going and a shortcut installed.
    void rebuildPageColumn();

    QuickButtonSet m_set;
    /// Which page is showing, counting from 1.
    int m_page = 1;
    /// Whether `setButtons` has laid the panel out once. Until it has, an
    /// unchanged list is still a rebuild worth doing — see `setButtons`.
    bool m_built = false;
    /// One per button, on every page — so a shortcut belongs to the window and
    /// not to whichever page happens to be showing.
    QVector<QAction *> m_actions;
    /// The same length, and **null for a button that is not on this page**.
    /// Every index in this class is a position in the whole list.
    QVector<QToolButton *> m_widgets;
    /// Per button: sends left, -1 for a run with no end, 0 for not running.
    QVector<int> m_remaining;
    QAction *m_add = nullptr;
    QBoxLayout *m_layout = nullptr;
    QFrame *m_separator = nullptr;
    /// The page drop-down, or null when there is only one page — the same rule
    /// that keeps the bar itself out of the way until a button exists.
    QComboBox *m_pageBox = nullptr;
    /// See `setFitted`.
    bool m_fitted = true;
};
