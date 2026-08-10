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
#include <QScrollArea>
#include <QSpinBox>
#include <QTabWidget>
#include <QVBoxLayout>

#include "Session.h"
#include "I18n.h"

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

QString pageTitle(const QString &page)
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

SettingsDialog::SettingsDialog(Session *session, I18n *i18n, QWidget *parent)
    : QDialog(parent), m_session(session), m_i18n(i18n)
{
    setWindowTitle(tr("Setup"));
    build();
}

void SettingsDialog::build()
{
    auto *layout = new QVBoxLayout(this);

    m_search = new QLineEdit(this);
    m_search->setPlaceholderText(tr("Search settings"));
    m_search->setClearButtonEnabled(true);
    connect(m_search, &QLineEdit::textChanged, this, &SettingsDialog::applyFilter);
    layout->addWidget(m_search);

    m_tabs = new QTabWidget(this);
    layout->addWidget(m_tabs, 1);

    // One tab per page, created in the order the schema lists them, so the
    // schema decides the layout as well as the content.
    QHash<QString, QFormLayout *> pages;

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
        if (!tt_settings_field(i, &f)) {
            continue;
        }
        const QString name = QString::fromUtf8(f.name);
        const QString page = QString::fromUtf8(f.page);
        const QString current = m_session->setting(name);

        QFormLayout *form = pages.value(page);
        if (!form) {
            // A scroll area per tab, because the pages are already taller than
            // a laptop screen at 39 settings and this is meant to hold 600.
            auto *body = new QWidget(m_tabs);
            form = new QFormLayout(body);
            form->setFieldGrowthPolicy(QFormLayout::ExpandingFieldsGrow);
            auto *scroll = new QScrollArea(m_tabs);
            scroll->setWidgetResizable(true);
            scroll->setWidget(body);
            m_tabs->addTab(scroll, pageTitle(page));
            pages.insert(page, form);
        }

        Row row;
        row.name = name;
        row.page = page;
        row.original = current;
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
        row.haystack = (name + QLatin1Char(' ') + row.label->text() + QLatin1Char(' ')
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
            if (f.kind == TT_SETTING_KIND_INT_RANGE) {
                spin->setRange(f.min, f.max);
            } else {
                spin->setRange(0, 1000000);
            }
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

        row.editor->setToolTip(row.label->toolTip());
        row.label->setBuddy(row.editor);
        form->addRow(row.label, row.editor);
        m_rows.push_back(row);
    }

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel,
                                         this);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] {
        applyChanges();
        accept();
    });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
    layout->addWidget(buttons);

    resize(640, 520);
}

void SettingsDialog::applyChanges()
{
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
        if (!m_session->setSetting(row.name, value, &error)) {
            failures << QStringLiteral("%1: %2").arg(row.name, error);
        }
    }
    if (!failures.isEmpty()) {
        QMessageBox::warning(this, tr("Setup"), failures.join(QLatin1Char('\n')));
    }
}

void SettingsDialog::applyFilter(const QString &text)
{
    const QString needle = text.trimmed().toLower();
    for (const Row &row : m_rows) {
        const bool show = needle.isEmpty() || row.haystack.contains(needle);
        row.label->setVisible(show);
        row.editor->setVisible(show);
    }
}
