// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "SettingsDialog.h"

#include <QCheckBox>
#include <QColorDialog>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QPushButton>
#include <QFrame>
#include <QScrollArea>
#include <QSet>
#include <QSpinBox>
#include <QStackedWidget>
#include <QVBoxLayout>

#include "Session.h"
#include "I18n.h"
#include "Plugins.h"
#include "TabRows.h"

namespace {

/// `terminal.scrollback_lines` → "Scrollback lines".
///
/// The source-language fallback. It also happens to be what a user searching
/// for a setting types, which the label of a translated dialog is not.
QString humanise(const QString &name)
{
    QString text = name.section(QLatin1Char('.'), 1);
    text.replace(QLatin1Char('_'), QLatin1Char(' '));
    text.replace(QLatin1Char('.'), QLatin1Char(' '));
    if (!text.isEmpty()) {
        text[0] = text[0].toUpper();
    }
    return text;
}

QString displayPageTitle(const QString &page)
{
    QString text = page;
    if (!text.isEmpty()) {
        text[0] = text[0].toUpper();
    }
    return text;
}

/// The tooltip: what the setting is, where it lives in the file, and the
/// citation for its default.
///
/// The citation is the part worth carrying all the way to the UI. Four of
/// these defaults are an `else` branch or a flag word built up a thousand
/// lines from where it was zeroed, and `AGENTS.md` has a trap written about
/// each — so "why is this on?" has an answer in the dialog rather than in the
/// source.
QString tooltip(const TtSettingField &f)
{
    QString text = QStringLiteral("<b>%1</b><br>[%2] %3 = %4")
                       .arg(QString::fromUtf8(f.name),
                            QString::fromUtf8(f.section),
                            QString::fromUtf8(f.key),
                            QString::fromUtf8(f.default_value));
    if (f.label) {
        text += QStringLiteral("<br>%1").arg(QString::fromUtf8(f.label));
    }
    const QString doc = QString::fromUtf8(f.doc);
    if (!doc.isEmpty()) {
        text += QStringLiteral("<hr>%1").arg(doc.toHtmlEscaped());
    }
    return text;
}

/// A button that shows a colour and opens a picker. The colour lives on the
/// button as a property, so the row's `value` lambda can read it back without
/// a widget subclass or a signal.
QPushButton *colorButton(const QColor &initial, QWidget *parent)
{
    auto *button = new QPushButton(parent);
    auto paint = [button](const QColor &c) {
        button->setProperty("ttColor", c);
        button->setText(c.name().toUpper());
        // Foreground chosen off the luminance so the hex code stays readable
        // on both a white and a black swatch.
        const QString fg = c.lightnessF() > 0.5 ? QStringLiteral("#000")
                                                : QStringLiteral("#fff");
        button->setStyleSheet(
            QStringLiteral("background-color: %1; color: %2;").arg(c.name(), fg));
    };
    paint(initial);
    QObject::connect(button, &QPushButton::clicked, button, [button, paint] {
        const QColor current = button->property("ttColor").value<QColor>();
        const QColor chosen = QColorDialog::getColor(current, button);
        if (chosen.isValid()) {
            paint(chosen);
        }
    });
    return button;
}

QColor colorOf(const QPushButton *button)
{
    return button->property("ttColor").value<QColor>();
}

/// The six numbers a `color2` value is, back out of two buttons.
QString pairValue(const QPushButton *fg, const QPushButton *bg)
{
    const QColor f = colorOf(fg);
    const QColor b = colorOf(bg);
    return QStringLiteral("%1,%2,%3,%4,%5,%6")
        .arg(f.red())
        .arg(f.green())
        .arg(f.blue())
        .arg(b.red())
        .arg(b.green())
        .arg(b.blue());
}

QColor colorAt(const QStringList &parts, int i)
{
    if (parts.size() < i + 3) {
        return QColor(Qt::black);
    }
    return QColor(parts.at(i).trimmed().toInt(), parts.at(i + 1).trimmed().toInt(),
                  parts.at(i + 2).trimmed().toInt());
}

} // namespace

SettingsDialog::SettingsDialog(Session *session, Plugins *plugins, I18n *i18n,
                               QWidget *parent, int initialPage)
    : QDialog(parent)
    , m_session(session)
    , m_plugins(plugins)
    , m_i18n(i18n)
    , m_initialPage(initialPage)
{
    setWindowTitle(m_i18n ? m_i18n->plainText("MENU_SETUP", tr("Setup"))
                          : tr("Setup"));
    build();
}

QVector<SettingsDialog::Page> SettingsDialog::corePages()
{
    QVector<Page> pages;
    QSet<QString> seen;
    for (size_t i = 0, count = tt_settings_field_count(); i < count; i++) {
        TtSettingField field;
        if (!tt_settings_field(i, &field) || !field.page) {
            continue;
        }
        const QString id = QString::fromUtf8(field.page);
        if (!seen.contains(id)) {
            seen.insert(id);
            pages.append({id, displayPageTitle(id)});
        }
    }
    return pages;
}

void SettingsDialog::build()
{
    auto *layout = new QVBoxLayout(this);

    m_search = new QLineEdit(this);
    m_search->setObjectName(QStringLiteral("settingsSearch"));
    m_search->setPlaceholderText(tr("Search settings"));
    m_search->setClearButtonEnabled(true);
    connect(m_search, &QLineEdit::textChanged, this, &SettingsDialog::applyFilter);
    layout->addWidget(m_search);

    // The tabs and the pages are two widgets rather than a `QTabWidget`,
    // because the bar has to wrap: the schema has 26 pages and a `QTabBar`
    // puts them on one line behind scroll buttons. See `TabRows`.
    m_tabs = new TabRows(this);
    m_pages = new QStackedWidget(this);
    m_pages->setFrameShape(QFrame::StyledPanel);
    connect(m_tabs, &TabRows::currentChanged, m_pages,
            &QStackedWidget::setCurrentIndex);
    layout->addWidget(m_tabs);
    layout->addWidget(m_pages, 1);
    m_noResults = new QLabel(tr("No settings match your search."), this);
    m_noResults->setObjectName(QStringLiteral("settingsNoResultsLabel"));
    m_noResults->setAlignment(Qt::AlignCenter);
    m_noResults->hide();
    layout->addWidget(m_noResults, 1);

    // One tab per page, created in the order the schema lists them, so the
    // schema decides the layout as well as the content.
    QHash<QString, QFormLayout *> pages;
    QHash<QString, int> pageIndices;

    // Some schema rows share an upstream label because it names a group rather
    // than either value inside it — foreground/background colour pairs and
    // terminal dimensions, for example. Translating that key onto every row
    // would make distinct settings display the same name. Only a unique key is
    // a field label; the rest keep their unambiguous generated fallback.
    QHash<QString, int> labelUses;
    const size_t count = tt_settings_field_count();
    for (size_t i = 0; i < count; i++) {
        TtSettingField field;
        if (tt_settings_field(i, &field) && field.label) {
            labelUses[QString::fromUtf8(field.label)]++;
        }
    }

    for (size_t i = 0; i < count; i++) {
        TtSettingField f;
        // The same skip `corePages` applies, and it has to be the same one:
        // the Setup menu addresses these tabs by index, so a field with no
        // page that produced a tab here and none there would silently move
        // every later page's action onto the wrong tab. `QString::fromUtf8`
        // on a null gives an empty string rather than refusing, so without
        // this the drift has nothing to announce it.
        if (!tt_settings_field(i, &f) || !f.page) {
            continue;
        }
        const QString name = QString::fromUtf8(f.name);
        const QString page = QString::fromUtf8(f.page);
        const QString current = m_session->setting(name);

        QFormLayout *form = pages.value(page);
        if (!form) {
            form = addPage(displayPageTitle(page));
            pages.insert(page, form);
            pageIndices.insert(page, m_tabs->count() - 1);
        }

        Row row;
        row.name = name;
        row.page = page;
        row.tab = pageIndices.value(page);
        row.apply = [this, name](const QString &value, QString *error) {
            return m_session->setSetting(name, value, error);
        };
        const QString fallback = humanise(name);
        QString label = fallback;
        if (m_i18n && f.label) {
            const QString key = QString::fromUtf8(f.label);
            if (labelUses.value(key) == 1) {
                label = m_i18n->text(f.label, fallback);
            }
        }
        row.label = new QLabel(label, this);
        row.label->setToolTip(tooltip(f));
        row.haystack = (name + QLatin1Char(' ') + displayPageTitle(page)
                        + QLatin1Char(' ') + row.label->text() + QLatin1Char(' ')
                        + QString::fromUtf8(f.key) + QLatin1Char(' ')
                        + QString::fromUtf8(f.doc))
                           .toLower();

        switch (f.kind) {
        case TT_SETTING_KIND_BOOL: {
            // The value is `on`/`off` because that is what the file says, and
            // the schema normalises it on the way out — so this is a
            // comparison, not a second copy of `GetOnOff`'s default-biased
            // parse.
            auto *box = new QCheckBox(this);
            box->setChecked(current == QLatin1String("on"));
            row.editor = box;
            row.value = [box] {
                return box->isChecked() ? QStringLiteral("on") : QStringLiteral("off");
            };
            break;
        }
        case TT_SETTING_KIND_INT:
        case TT_SETTING_KIND_INT_RANGE: {
            auto *spin = new QSpinBox(this);
            // Both kinds carry a usable pair — an unbounded `Int` reports
            // `i32::MIN`/`i32::MAX` — and taking it is load-bearing rather
            // than tidy. Six settings ship a negative sentinel:
            // `serial.rts`/`serial.dtr` are -1 for "derive from `ts.Flow`",
            // and `window.x/y`, `tek.x/y` are `CW_USEDEFAULT`. A range
            // starting at 0 shows every one of them as 0, and since `original`
            // is captured from the editor, the user can then never commit a
            // real 0 — the box already reads 0, so OK sees no change.
            spin->setRange(f.min, f.max);
            spin->setValue(current.toInt());
            row.editor = spin;
            row.value = [spin] { return QString::number(spin->value()); };
            break;
        }
        case TT_SETTING_KIND_ENUM: {
            auto *combo = new QComboBox(this);
            for (size_t c = 0; c < f.choices; c++) {
                const char *spelling = tt_settings_choice(i, c);
                if (spelling) {
                    combo->addItem(QString::fromUtf8(spelling));
                }
            }
            // The INI's own spellings, both ways: no display/value mapping to
            // get wrong, and `TerminalID` is compared case-sensitively
            // upstream, so a prettified spelling would silently read as the
            // default.
            const int at = combo->findText(current);
            combo->setCurrentIndex(at >= 0 ? at : 0);
            row.editor = combo;
            row.value = [combo] { return combo->currentText(); };
            break;
        }
        case TT_SETTING_KIND_COLOR2: {
            const QStringList parts = current.split(QLatin1Char(','));
            auto *box = new QWidget(this);
            auto *hbox = new QHBoxLayout(box);
            hbox->setContentsMargins(0, 0, 0, 0);
            auto *fg = colorButton(colorAt(parts, 0), box);
            auto *bg = colorButton(colorAt(parts, 3), box);
            hbox->addWidget(new QLabel(tr("Text"), box));
            hbox->addWidget(fg, 1);
            hbox->addWidget(new QLabel(tr("Background"), box));
            hbox->addWidget(bg, 1);
            row.editor = box;
            row.value = [fg, bg] { return pairValue(fg, bg); };
            break;
        }
        case TT_SETTING_KIND_STR:
        default: {
            if (name == QLatin1String("settings.language_file")) {
                auto *combo = new QComboBox(this);
                for (const LanguageChoice &language : I18n::availableLanguages()) {
                    combo->addItem(language.name, language.setting);
                }
                int at = combo->findData(current);
                if (at < 0) {
                    combo->insertItem(0, current, current);
                    at = 0;
                }
                combo->setCurrentIndex(at);
                row.editor = combo;
                row.value = [combo] { return combo->currentData().toString(); };
                break;
            }
            auto *edit = new QLineEdit(current, this);
            row.editor = edit;
            row.value = [edit] { return edit->text(); };
            break;
        }
        }

        // Compare against what the editor actually opened with. Some legacy
        // sentinel or unknown values cannot be represented by a spin/combo
        // box and Qt normalises those while constructing it; that is not a
        // user edit and must not become a live change or an automatic write.
        row.original = row.value();
        row.editor->setObjectName(QStringLiteral("settingEditor:%1").arg(name));
        row.editor->setToolTip(row.label->toolTip());
        row.label->setBuddy(row.editor);
        form->addRow(row.label, row.editor);
        m_rows.push_back(row);
    }

    if (m_plugins) {
        for (const PluginSettingInfo &f : m_plugins->settings()) {
            const QString page = QStringLiteral("plugin:%1").arg(f.pageId);
            const QString current = m_plugins->setting(f.id);

            QFormLayout *form = pages.value(page);
            if (!form) {
                form = addPage(f.page);
                pages.insert(page, form);
                pageIndices.insert(page, m_tabs->count() - 1);
            }

            Row row;
            row.name = QStringLiteral("[%1] %2").arg(f.section, f.key);
            row.page = page;
            row.tab = pageIndices.value(page);
            row.plugin = true;
            row.pluginId = f.id;
            row.label = new QLabel(f.label, this);
            QString tip = QStringLiteral("<b>%1</b><br>%2<br>[%3] %4 = %5")
                              .arg(f.plugin.toHtmlEscaped(), f.name.toHtmlEscaped(),
                                   f.section.toHtmlEscaped(), f.key.toHtmlEscaped(),
                                   f.defaultValue.toHtmlEscaped());
            if (!f.description.isEmpty()) {
                tip += QStringLiteral("<hr>%1").arg(f.description.toHtmlEscaped());
            }
            row.label->setToolTip(tip);
            row.haystack = (f.plugin + QLatin1Char(' ') + f.page + QLatin1Char(' ')
                            + f.section + QLatin1Char(' ') + f.key + QLatin1Char(' ')
                            + f.name + QLatin1Char(' ') + f.label + QLatin1Char(' ')
                            + f.description)
                               .toLower();

            switch (f.kind) {
            case TT_SETTING_KIND_BOOL: {
                auto *box = new QCheckBox(this);
                box->setChecked(current == QLatin1String("on"));
                row.editor = box;
                row.value = [box] {
                    return box->isChecked() ? QStringLiteral("on")
                                            : QStringLiteral("off");
                };
                break;
            }
            case TT_SETTING_KIND_INT_RANGE: {
                auto *spin = new QSpinBox(this);
                spin->setRange(f.min, f.max);
                spin->setValue(current.toInt());
                row.editor = spin;
                row.value = [spin] { return QString::number(spin->value()); };
                break;
            }
            case TT_SETTING_KIND_ENUM: {
                auto *combo = new QComboBox(this);
                combo->addItems(f.choices);
                const int at = combo->findText(current);
                combo->setCurrentIndex(at >= 0 ? at : 0);
                row.editor = combo;
                row.value = [combo] { return combo->currentText(); };
                break;
            }
            case TT_SETTING_KIND_STR:
            default: {
                auto *edit = new QLineEdit(current, this);
                row.editor = edit;
                row.value = [edit] { return edit->text(); };
                break;
            }
            }

            row.original = row.value();
            const size_t id = f.id;
            row.apply = [this, id](const QString &value, QString *error) {
                return m_plugins->setSetting(id, value, error);
            };
            row.editor->setObjectName(QStringLiteral("luaPluginSetting%1").arg(id));
            row.editor->setToolTip(row.label->toolTip());
            row.label->setBuddy(row.editor);
            form->addRow(row.label, row.editor);
            m_rows.push_back(row);
        }
    }

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel,
                                         this);
    if (m_i18n) {
        buttons->button(QDialogButtonBox::Ok)
            ->setText(m_i18n->plainText("BTN_OK", tr("OK")));
        buttons->button(QDialogButtonBox::Cancel)
            ->setText(m_i18n->plainText("BTN_CANCEL", tr("Cancel")));
    }
    connect(buttons, &QDialogButtonBox::accepted, this, [this] {
        applyChanges();
        accept();
    });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addWidget(buttons);

    if (m_initialPage >= 0 && m_initialPage < m_tabs->count()) {
        m_tabs->setCurrentIndex(m_initialPage);
    }

    // Wide enough for the tabs to land on two rows rather than four, which is
    // what `TabRows` asks for: the page titles decide that width, so it comes
    // from the layout instead of a number here. Qt caps a window's initial size
    // at two thirds of the screen, which is the ceiling on it either way.
    resize(qMax(640, sizeHint().width()), 560);
}

QFormLayout *SettingsDialog::addPage(const QString &title)
{
    // A scroll area per tab, because the pages are already taller than a
    // laptop screen at 39 settings and this is meant to hold 600. Its own frame
    // is off: the stack draws the panel the tabs sit on.
    auto *body = new QWidget(m_pages);
    auto *form = new QFormLayout(body);
    form->setFieldGrowthPolicy(QFormLayout::ExpandingFieldsGrow);
    auto *scroll = new QScrollArea(m_pages);
    scroll->setWidgetResizable(true);
    scroll->setFrameShape(QFrame::NoFrame);
    scroll->setWidget(body);
    // The page before the tab, so the bar's first `currentChanged` — which it
    // emits as the first tab is added — has something to select.
    m_pages->addWidget(scroll);
    m_tabs->addTab(title);
    return form;
}

void SettingsDialog::applyChanges()
{
    m_appliedCoreChanges.clear();
    m_appliedPluginChanges.clear();
    QStringList failures;
    for (const Row &row : m_rows) {
        const QString value = row.value();
        // Only what changed. A dialog that wrote every field would pin all 39
        // settings into the user's file the first time it was opened, and a
        // pinned setting stops following upstream's default for ever.
        if (value == row.original) {
            continue;
        }
        QString error;
        if (!row.apply(value, &error)) {
            failures << QStringLiteral("%1: %2").arg(row.name, error);
        } else if (row.plugin) {
            m_appliedPluginChanges.append(row.pluginId);
        } else {
            m_appliedCoreChanges.append({row.name, value});
        }
    }
    if (!failures.isEmpty()) {
        QMessageBox::warning(this, tr("Setup"), failures.join(QLatin1Char('\n')));
    }
}

void SettingsDialog::applyFilter(const QString &text)
{
    const QString needle = text.trimmed().toLower();
    if (needle.isEmpty()) {
        for (const Row &row : m_rows) {
            row.label->show();
            row.editor->show();
        }
        for (int i = 0; i < m_tabs->count(); i++) {
            m_tabs->setTabVisible(i, true);
        }
        m_tabs->show();
        m_pages->show();
        m_noResults->hide();
        if (m_searchRestorePage >= 0) {
            m_tabs->setCurrentIndex(m_searchRestorePage);
            m_searchRestorePage = -1;
        }
        return;
    }

    if (m_searchRestorePage < 0) {
        m_searchRestorePage = m_tabs->currentIndex();
    }
    QVector<bool> matches(m_tabs->count(), false);
    for (const Row &row : m_rows) {
        const bool show = row.haystack.contains(needle);
        row.label->setVisible(show);
        row.editor->setVisible(show);
        if (show && row.tab >= 0 && row.tab < matches.size()) {
            matches[row.tab] = true;
        }
    }

    int first = -1;
    for (int i = 0; i < matches.size(); i++) {
        m_tabs->setTabVisible(i, matches.at(i));
        if (first < 0 && matches.at(i)) {
            first = i;
        }
    }
    const bool any = first >= 0;
    m_tabs->setVisible(any);
    m_pages->setVisible(any);
    m_noResults->setVisible(!any);
    if (any && !matches.at(m_tabs->currentIndex())) {
        m_tabs->setCurrentIndex(first);
    }
}
