// The settings dialog, which has no list of settings in it.
//
// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#pragma once

#include <QDialog>
#include <QString>
#include <QVector>

#include <functional>

#include "sterna.h"

class QFormLayout;
class QLabel;
class I18n;
class Plugins;
class QLineEdit;
class QStackedWidget;
class QWidget;
class Session;
class TabRows;

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
    struct Page {
        QString id;
        QString title;
    };

    explicit SettingsDialog(Session *session, Plugins *plugins = nullptr,
                            I18n *i18n = nullptr,
                            QWidget *parent = nullptr, int initialPage = 0);

    /// The built-in pages, in schema order. The Setup menu consumes this same
    /// list, so an action can never drift from the dialog tab it opens.
    static QVector<Page> corePages();

    /// Apply every changed row. Called on OK; public so a test can drive it
    /// without a button press.
    void applyChanges();
    const QVector<QPair<QString, QString>> &appliedCoreChanges() const
    {
        return m_appliedCoreChanges;
    }
    const QVector<size_t> &appliedPluginChanges() const
    {
        return m_appliedPluginChanges;
    }

private:
    /// One editable setting, and how to read what the user typed.
    struct Row {
        QString name;
        QString page;
        QString haystack;
        QLabel *label = nullptr;
        QWidget *editor = nullptr;
        /// What the editor showed when the dialog opened, so that OK touches
        /// only what the user changed. This is deliberately captured after Qt
        /// normalises values an editor cannot represent; merely opening such a
        /// row must not apply or persist that normalisation.
        QString original;
        int tab = -1;
        bool plugin = false;
        size_t pluginId = 0;
        std::function<QString()> value;
        std::function<bool(const QString &, QString *)> apply;
    };

    void build();
    /// Hide the rows that do not match, across every tab at once. The search
    /// box is the thing that makes 600 settings navigable, which is the number
    /// this dialog is eventually for.
    void applyFilter(const QString &text);
    /// One scrolling page and its tab. Returns the form to add rows to.
    QFormLayout *addPage(const QString &title);

    Session *m_session;
    Plugins *m_plugins;
    I18n *m_i18n;
    /// The tabs, on as many rows as the width needs — 26 schema pages do not
    /// fit on one, and a `QTabWidget` answers that with scroll buttons.
    TabRows *m_tabs = nullptr;
    QStackedWidget *m_pages = nullptr;
    QLineEdit *m_search = nullptr;
    QLabel *m_noResults = nullptr;
    QVector<Row> m_rows;
    QVector<QPair<QString, QString>> m_appliedCoreChanges;
    QVector<size_t> m_appliedPluginChanges;
    int m_initialPage = 0;
    int m_searchRestorePage = -1;
};
