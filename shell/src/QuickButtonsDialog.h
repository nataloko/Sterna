// Setup > Quick buttons — the editor for the bar's list.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QVector>

#include "QuickButtons.h"

class QCheckBox;
class QComboBox;
class QKeySequenceEdit;
class QLabel;
class QLineEdit;
class QListWidget;
class QPlainTextEdit;
class QPushButton;
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
    QuickButtonsDialog(const QVector<QuickButton> &buttons, const Session *session,
                       const QWidget *window, QWidget *parent = nullptr);

    /// The edited list. Only meaningful after `exec()` returned `Accepted`.
    QVector<QuickButton> buttons() const { return m_buttons; }

    /// Select a row, or append `seed` and select that. Called by the window
    /// for Edit, Add and New from selection.
    void selectRow(int index);
    void appendButton(const QuickButton &seed);

private:
    void rebuildList();
    /// Copy the fields into `m_buttons[m_current]`. Called on every edit, so
    /// that switching rows or pressing OK never loses what was typed.
    void commit();
    /// Point the fields at row `row`.
    void load(int row);
    void addButton();
    void duplicateButton();
    void removeButton();
    void assignDefaultShortcuts();
    /// Update the warning under the shortcut field.
    void checkShortcut();
    /// What is wrong with `sequence`, or an empty string.
    QString shortcutComplaint(const QKeySequence &sequence, int forRow) const;
    /// Which page of the value stack a kind wants.
    void applyKind();

    QVector<QuickButton> m_buttons;
    const Session *m_session = nullptr;
    const QWidget *m_window = nullptr;
    int m_current = -1;
    /// True while `load` is writing the widgets, so their change signals do
    /// not write straight back into the row being loaded.
    bool m_loading = false;

    QListWidget *m_list = nullptr;
    QLineEdit *m_label = nullptr;
    QComboBox *m_kind = nullptr;
    QStackedWidget *m_value = nullptr;
    QPlainTextEdit *m_text = nullptr;
    QLineEdit *m_path = nullptr;
    QComboBox *m_command = nullptr;
    QCheckBox *m_enter = nullptr;
    QKeySequenceEdit *m_shortcut = nullptr;
    QLabel *m_warning = nullptr;
    QCheckBox *m_confirm = nullptr;
    QWidget *m_fields = nullptr;
    QPushButton *m_duplicate = nullptr;
    QPushButton *m_remove = nullptr;
};
