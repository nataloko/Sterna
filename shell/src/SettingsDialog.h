// The settings dialog, which has no list of settings in it.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QString>
#include <QVector>

#include <functional>

#include "sterna.h"

class QLabel;
class I18n;
class Plugins;
class QLineEdit;
class QTabWidget;
class QWidget;
class Session;

/// Every setting, built from the core's metadata table.
///
/// **This is the leverage point `PLAN.md` names.** Tera Term surfaces a
/// 909-line settings struct through ~13.8k lines of dialog code across 76
/// dialog templates in 30 `.rc` files, and hand-porting that is where this
/// project would stop. So there is one declarative schema in `tt-config`, and
/// this walks `tt_settings_field` to decide what widgets exist: a tab per
/// page, a widget per kind, the `.lng` key and the citation for the default in
/// the tooltip.
///
/// A dialog *generated* as C++ was the original sketch and is worse: it would
/// be a second copy of the list, living in the other build system, that a
/// schema change has to be pushed through. Reading the metadata at runtime
/// leaves nothing to keep in step — adding a setting is a line in
/// `schema/settings.txt` and a widget appears.
///
class SettingsDialog : public QDialog {
    Q_OBJECT

public:
    explicit SettingsDialog(Session *session, Plugins *plugins = nullptr,
                            I18n *i18n = nullptr,
                            QWidget *parent = nullptr);

    /// Apply every changed row. Called on OK; public so a test can drive it
    /// without a button press.
    void applyChanges();

private:
    /// One editable setting, and how to read what the user typed.
    struct Row {
        QString name;
        QString page;
        QString haystack;
        QLabel *label = nullptr;
        QWidget *editor = nullptr;
        /// What the setting held when the dialog opened, so that OK touches
        /// only what changed — a setting nobody looked at must not be
        /// rewritten, since writing one is what pins a default that might
        /// otherwise follow upstream.
        QString original;
        std::function<QString()> value;
        std::function<bool(const QString &, QString *)> apply;
    };

    void build();
    /// Hide the rows that do not match, across every tab at once. The search
    /// box is the thing that makes 600 settings navigable, which is the number
    /// this dialog is eventually for.
    void applyFilter(const QString &text);

    Session *m_session;
    Plugins *m_plugins;
    I18n *m_i18n;
    QTabWidget *m_tabs = nullptr;
    QLineEdit *m_search = nullptr;
    QVector<Row> m_rows;
};
