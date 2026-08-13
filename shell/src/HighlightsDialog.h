// The highlight rule editor.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QVector>

#include "Highlights.h"

class QCheckBox;
class QLabel;
class QLineEdit;
class QListWidget;
class QPlainTextEdit;
class QPushButton;
class QSpinBox;
class QWidget;

/// Setup > Highlighting.
///
/// A dialog of its own rather than a page in `SettingsDialog`: that one is
/// generated from the settings schema, and a list is exactly what the schema
/// cannot describe — so a table in the middle of it would be a special case in
/// the one place whose value is that it has none.
///
/// It edits a copy. Cancel therefore costs nothing, and the window writes the
/// file — the dialog never touches it.
class HighlightsDialog : public QDialog {
    Q_OBJECT

public:
    explicit HighlightsDialog(const QVector<QuickHighlight> &rules, QWidget *parent = nullptr);

    /// What to save. Only meaningful after `exec()` returned `Accepted`.
    const QVector<QuickHighlight> &rules() const { return m_rules; }

private:
    /// Refill the list from `m_rules`, keeping the current row.
    void rebuildList();
    /// Point the fields at row `row`; a negative row disables them.
    void load(int row);
    /// Copy the fields back into the current row. Wired to every field's change
    /// signal, so the list's caption follows the label and nothing has to be
    /// applied before switching rows.
    void commit();
    /// Refuse OK while any non-empty pattern is invalid, selecting the first
    /// rule that needs attention.
    bool validatePatterns();
    /// Re-check the pattern and repaint the sample, which is the same work
    /// whichever field moved.
    void refreshPreview();
    void addRule(const QuickHighlight &rule);

    QVector<QuickHighlight> m_rules;
    int m_current = -1;
    /// True while `load` is writing the widgets, so their change signals do not
    /// write straight back into the row being loaded.
    bool m_loading = false;

    QListWidget *m_list = nullptr;
    QPushButton *m_duplicate = nullptr;
    QPushButton *m_remove = nullptr;
    QWidget *m_fields = nullptr;

    QLineEdit *m_label = nullptr;
    QLineEdit *m_pattern = nullptr;
    QLabel *m_patternError = nullptr;
    QCheckBox *m_literal = nullptr;
    QCheckBox *m_ignoreCase = nullptr;
    /// Whether the rule touches that channel at all. A colour picker cannot
    /// express "leave this one alone", and that is the state a rule which
    /// changes only the background is in.
    QCheckBox *m_foreOn = nullptr;
    QCheckBox *m_backOn = nullptr;
    QPushButton *m_fore = nullptr;
    QPushButton *m_back = nullptr;
    QCheckBox *m_bold = nullptr;
    QCheckBox *m_underline = nullptr;
    QCheckBox *m_reverse = nullptr;
    QCheckBox *m_wholeLine = nullptr;
    QSpinBox *m_group = nullptr;
    QPlainTextEdit *m_sample = nullptr;
    QLabel *m_preview = nullptr;
};
