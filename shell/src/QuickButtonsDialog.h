// Setup > Quick buttons — the editor for the bar's list.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QVector>

#include "QuickButtons.h"

class QAction;
class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QKeySequenceEdit;
class QLabel;
class QLineEdit;
class QListWidget;
class QPlainTextEdit;
class QPushButton;
class QSpinBox;
class QStackedWidget;
class Session;

/// The list on the left, the button's fields on the right.
///
/// A dialog of its own rather than a page in `SettingsDialog`: that one is
/// generated from the settings schema and a list is exactly what the schema
/// cannot describe, so a table in the middle of it would be a special case in
/// the one place whose value is that it has none.
///
/// It edits a copy. Cancel therefore costs nothing, and the window writes the
/// file — the dialog never touches it.
class QuickButtonsDialog : public QDialog {
    Q_OBJECT

public:
    /// `session` is asked whether a shortcut is already a `KEYBOARD.CNF` key,
    /// and `window` for the shortcuts its own actions and Lua plugins hold.
    /// Both may be null in a test that only cares about the list.
    ///
    /// `page` is the one the panel is showing, so the editor opens where the
    /// user was rather than on the first page.
    QuickButtonsDialog(const QuickButtonSet &set, int page, const Session *session,
                       const QWidget *window, QWidget *parent = nullptr);

    /// The edited section. Only meaningful after `exec()` returned `Accepted`.
    QuickButtonSet set() const { return m_set; }
    QVector<QuickButton> buttons() const { return m_set.buttons; }

    /// Select a row, or append `seed` and select that. Called by the window
    /// for Edit, Add and New from selection.
    void selectRow(int index);
    void appendButton(const QuickButton &seed);

private:
    void rebuildList();
    /// Copy the fields into `m_set.buttons[m_current]`. Called on every edit,
    /// so that switching rows or pressing OK never loses what was typed.
    void commit();
    /// Point the fields at list row `row`.
    void load(int row);
    void addButton();
    void duplicateButton();
    void removeButton();
    void assignDefaultShortcuts();

    // --- pages --------------------------------------------------------------
    //
    // **The list is filtered to one page, so a row is no longer an index.**
    // `m_current` stays the *global* one, because everything else in this
    // dialog — the shortcut check, duplicate, remove — is about a button's
    // place in the whole list. `m_rows` is the only place the two meet.
    int globalOf(int row) const { return m_rows.value(row, -1); }
    int rowOf(int global) const { return static_cast<int>(m_rows.indexOf(global)); }
    void rebuildPages();
    void setPage(int page);
    void addPage();
    void renamePage();
    void removePage();
    void importPage();
    void exportPage();
    /// Show or hide the page row, which is worth nothing until there is a
    /// second page to move between.
    void applyPageControls();
    /// Update the warning under the shortcut field.
    void checkShortcut();
    /// What is wrong with `sequence`, or an empty string.
    QString shortcutComplaint(const QKeySequence &sequence, int forRow) const;
    /// Which page of the value stack a kind wants.
    void applyKind();
    /// Show or hide the parts of the repeat row that only mean something for
    /// a button that sends more than once.
    void applyRepeat();

    QuickButtonSet m_set;
    /// Which page the list is filtered to, counting from 1.
    int m_page = 1;
    /// Global indices of the rows on show, in row order.
    QVector<int> m_rows;
    const Session *m_session = nullptr;
    const QWidget *m_window = nullptr;
    /// The **global** index of the button the fields are showing, or -1.
    int m_current = -1;
    /// True while `load` is writing the widgets, so their change signals do
    /// not write straight back into the row being loaded.
    bool m_loading = false;

    QListWidget *m_list = nullptr;
    QWidget *m_pageRow = nullptr;
    QComboBox *m_pageList = nullptr;
    /// The button a button is on. In the fields rather than only in the panel's
    /// menu, because moving one is an edit like any other.
    QComboBox *m_pageOf = nullptr;
    QLineEdit *m_label = nullptr;
    QComboBox *m_kind = nullptr;
    QStackedWidget *m_value = nullptr;
    QPlainTextEdit *m_text = nullptr;
    QLineEdit *m_path = nullptr;
    QComboBox *m_command = nullptr;
    QCheckBox *m_enter = nullptr;
    /// Sends per press. Its minimum, 0, is shown as "Until stopped" and means
    /// `TT_QUICK_BUTTON_REPEAT_FOREVER` — the count below one, since a run
    /// with no end is what you reach for when no number is right.
    QSpinBox *m_repeat = nullptr;
    /// "time" or "times", and gone entirely when there is no count to agree
    /// with. One row rather than two so the whole sentence is readable at a
    /// glance: *10 times every 2.5 s*.
    QLabel *m_repeatTimes = nullptr;
    QLabel *m_every = nullptr;
    QDoubleSpinBox *m_interval = nullptr;
    QKeySequenceEdit *m_shortcut = nullptr;
    QLabel *m_warning = nullptr;
    QCheckBox *m_confirm = nullptr;
    QWidget *m_fields = nullptr;
    QPushButton *m_duplicate = nullptr;
    QPushButton *m_remove = nullptr;
    /// Disabled while there is only one page: there is nowhere for its buttons
    /// to go, and a page nobody can leave is not one to remove.
    QAction *m_pageRemove = nullptr;
};
