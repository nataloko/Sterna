// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "HighlightsDialog.h"

#include <QCheckBox>
#include <QColorDialog>
#include <QDialogButtonBox>
#include <QFontDatabase>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

#include <utility>

namespace {

/// A button that shows a colour and opens a picker, with an "off" state for
/// the channel a rule leaves alone.
///
/// The colour lives on the button as a property, the way `SettingsDialog`'s
/// does, so the fields can be read back without a widget subclass.
QPushButton *colorButton(QWidget *parent)
{
    auto *button = new QPushButton(parent);
    QObject::connect(button, &QPushButton::clicked, button, [button] {
        const QColor current = button->property("ttColor").value<QColor>();
        const QColor chosen =
            QColorDialog::getColor(current.isValid() ? current : QColor(Qt::white), button);
        if (chosen.isValid()) {
            button->setProperty("ttColor", chosen);
            // The button repaints itself through the same code the loader uses,
            // so there is one place that knows what "off" looks like.
            Q_EMIT button->toggled(true);
        }
    });
    return button;
}

void paintColorButton(QPushButton *button, const QColor &color)
{
    button->setProperty("ttColor", color);
    if (!color.isValid()) {
        button->setText(QObject::tr("Unchanged"));
        button->setStyleSheet(QString());
        return;
    }
    button->setText(color.name().toUpper());
    // Foreground off the luminance, so the hex code stays readable on both a
    // white and a black swatch.
    const QString fg = color.lightnessF() > 0.5 ? QStringLiteral("#000") : QStringLiteral("#fff");
    button->setStyleSheet(
        QStringLiteral("background-color: %1; color: %2;").arg(color.name(), fg));
}

QColor colorOf(const QPushButton *button)
{
    return button->property("ttColor").value<QColor>();
}

} // namespace

HighlightsDialog::HighlightsDialog(const QVector<QuickHighlight> &rules, QWidget *parent)
    : QDialog(parent), m_rules(rules)
{
    setWindowTitle(tr("Highlighting"));
    setObjectName(QStringLiteral("highlightsDialog"));

    m_list = new QListWidget(this);
    m_list->setObjectName(QStringLiteral("highlightList"));
    // A drag reorders, and order is priority — the first rule to claim a
    // cell's foreground keeps it. A pair of up/down buttons would say the same
    // thing in two more widgets.
    m_list->setDragDropMode(QAbstractItemView::InternalMove);
    m_list->setMinimumWidth(200);

    auto *add = new QPushButton(tr("Add"), this);
    add->setObjectName(QStringLiteral("highlightAdd"));
    m_duplicate = new QPushButton(tr("Duplicate"), this);
    m_duplicate->setObjectName(QStringLiteral("highlightDuplicate"));
    m_remove = new QPushButton(tr("Remove"), this);
    m_remove->setObjectName(QStringLiteral("highlightRemove"));

    auto *listButtons = new QHBoxLayout;
    listButtons->addWidget(add);
    listButtons->addWidget(m_duplicate);
    listButtons->addWidget(m_remove);

    auto *left = new QVBoxLayout;
    left->addWidget(m_list);
    left->addLayout(listButtons);

    m_fields = new QWidget(this);
    auto *form = new QFormLayout(m_fields);

    m_label = new QLineEdit(m_fields);
    m_label->setObjectName(QStringLiteral("highlightLabel"));
    m_label->setPlaceholderText(tr("optional"));
    form->addRow(tr("Name"), m_label);

    m_pattern = new QLineEdit(m_fields);
    m_pattern->setObjectName(QStringLiteral("highlightPattern"));
    form->addRow(tr("Matches"), m_pattern);

    m_patternError = new QLabel(m_fields);
    m_patternError->setObjectName(QStringLiteral("highlightPatternError"));
    m_patternError->setStyleSheet(QStringLiteral("color: #c00;"));
    m_patternError->setWordWrap(true);
    form->addRow(QString(), m_patternError);

    m_literal = new QCheckBox(tr("Plain text, not a pattern"), m_fields);
    m_literal->setObjectName(QStringLiteral("highlightLiteral"));
    form->addRow(QString(), m_literal);
    m_ignoreCase = new QCheckBox(tr("Ignore case"), m_fields);
    m_ignoreCase->setObjectName(QStringLiteral("highlightIgnoreCase"));
    form->addRow(QString(), m_ignoreCase);

    // A check box beside each colour because "leave this one alone" is a real
    // answer and a colour picker cannot express it — that is what lets a rule
    // change only the background.
    m_foreOn = new QCheckBox(m_fields);
    m_foreOn->setObjectName(QStringLiteral("highlightForeOn"));
    m_fore = colorButton(m_fields);
    m_fore->setObjectName(QStringLiteral("highlightFore"));
    auto *foreRow = new QHBoxLayout;
    foreRow->addWidget(m_foreOn);
    foreRow->addWidget(m_fore, 1);
    form->addRow(tr("Text colour"), foreRow);

    m_backOn = new QCheckBox(m_fields);
    m_backOn->setObjectName(QStringLiteral("highlightBackOn"));
    m_back = colorButton(m_fields);
    m_back->setObjectName(QStringLiteral("highlightBack"));
    auto *backRow = new QHBoxLayout;
    backRow->addWidget(m_backOn);
    backRow->addWidget(m_back, 1);
    form->addRow(tr("Background"), backRow);

    m_bold = new QCheckBox(tr("Bold"), m_fields);
    m_bold->setObjectName(QStringLiteral("highlightBold"));
    m_underline = new QCheckBox(tr("Underline"), m_fields);
    m_underline->setObjectName(QStringLiteral("highlightUnderline"));
    m_reverse = new QCheckBox(tr("Reverse"), m_fields);
    m_reverse->setObjectName(QStringLiteral("highlightReverse"));
    auto *styleRow = new QHBoxLayout;
    styleRow->addWidget(m_bold);
    styleRow->addWidget(m_underline);
    styleRow->addWidget(m_reverse);
    styleRow->addStretch(1);
    form->addRow(tr("Style"), styleRow);

    m_wholeLine = new QCheckBox(tr("Colour the whole line"), m_fields);
    m_wholeLine->setObjectName(QStringLiteral("highlightWholeLine"));
    form->addRow(QString(), m_wholeLine);

    m_group = new QSpinBox(m_fields);
    m_group->setObjectName(QStringLiteral("highlightGroup"));
    m_group->setRange(0, 99);
    m_group->setSpecialValueText(tr("Entire match"));
    m_group->setPrefix(tr("Capture group "));
    m_group->setToolTip(
        tr("The style usually changes the full match. A capture group changes "
           "only the text in its numbered parentheses."));
    form->addRow(tr("Apply to"), m_group);

    auto *groupHelp = new QLabel(
        tr("Parentheses divide a pattern into numbered parts. Choose one to "
           "style only that part."),
        m_fields);
    groupHelp->setObjectName(QStringLiteral("highlightGroupHelp"));
    groupHelp->setWordWrap(true);
    form->addRow(QString(), groupHelp);

    // The sample box, which is the thing that makes writing a pattern
    // bearable: the line is coloured by the same engine the terminal uses, so
    // what is shown here is what will happen.
    m_sample = new QPlainTextEdit(m_fields);
    m_sample->setObjectName(QStringLiteral("highlightSample"));
    m_sample->setPlaceholderText(tr("Type a line to try the rules on"));
    m_sample->setMaximumHeight(60);
    form->addRow(tr("Sample"), m_sample);

    m_preview = new QLabel(m_fields);
    m_preview->setObjectName(QStringLiteral("highlightPreview"));
    m_preview->setTextFormat(Qt::RichText);
    m_preview->setWordWrap(true);
    m_preview->setFont(QFontDatabase::systemFont(QFontDatabase::FixedFont));
    form->addRow(QString(), m_preview);

    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(buttons, &QDialogButtonBox::accepted, this, [this] {
        commit();
        if (validatePatterns()) {
            accept();
        }
    });
    connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *columns = new QHBoxLayout;
    columns->addLayout(left, 1);
    columns->addWidget(m_fields, 2);
    auto *root = new QVBoxLayout(this);
    root->addLayout(columns);
    root->addWidget(buttons);

    connect(m_list, &QListWidget::currentRowChanged, this, [this](int row) {
        if (m_loading) {
            return;
        }
        commit();
        load(row);
    });
    // The check box on each row is the rule's own switch, so it can be turned
    // off without being deleted and retyped.
    connect(m_list, &QListWidget::itemChanged, this, [this](QListWidgetItem *item) {
        if (m_loading) {
            return;
        }
        const int row = m_list->row(item);
        if (row >= 0 && row < m_rules.size()) {
            m_rules[row].enabled = item->checkState() == Qt::Checked;
        }
    });
    connect(m_list->model(), &QAbstractItemModel::rowsMoved, this,
            [this](const QModelIndex &, int from, int, const QModelIndex &, int to) {
                if (m_loading || from < 0 || from >= m_rules.size()) {
                    return;
                }
                m_rules.move(from, to > from ? to - 1 : to);
                m_current = m_list->currentRow();
            });

    connect(add, &QPushButton::clicked, this, [this] { addRule(QuickHighlight()); });
    connect(m_duplicate, &QPushButton::clicked, this, [this] {
        if (m_current >= 0 && m_current < m_rules.size()) {
            commit();
            addRule(m_rules.at(m_current));
        }
    });
    connect(m_remove, &QPushButton::clicked, this, [this] {
        if (m_current < 0 || m_current >= m_rules.size()) {
            return;
        }
        const int row = m_current;
        m_rules.remove(row);
        m_current = -1;
        rebuildList();
        const int next = qMin(row, m_rules.size() - 1);
        if (next >= 0) {
            m_list->setCurrentRow(next);
        } else {
            load(-1);
        }
    });

    // Every field writes back as it is typed, so the list's caption follows the
    // name and nothing has to be applied before switching rows.
    for (QLineEdit *edit : {m_label, m_pattern}) {
        connect(edit, &QLineEdit::textChanged, this, [this] { commit(); });
    }
    for (QCheckBox *box : {m_literal, m_ignoreCase, m_bold, m_underline, m_reverse,
                           m_wholeLine, m_foreOn, m_backOn}) {
        connect(box, &QCheckBox::toggled, this, [this] { commit(); });
    }
    for (QPushButton *button : {m_fore, m_back}) {
        connect(button, &QPushButton::toggled, this, [this] { commit(); });
    }
    connect(m_group, QOverload<int>::of(&QSpinBox::valueChanged), this, [this] { commit(); });
    connect(m_sample, &QPlainTextEdit::textChanged, this, [this] { refreshPreview(); });

    rebuildList();
    if (m_rules.isEmpty()) {
        load(-1);
    } else {
        // Through the list, so the row that is shown is the row that looks
        // selected.
        m_list->setCurrentRow(0);
    }
}

void HighlightsDialog::addRule(const QuickHighlight &rule)
{
    if (m_rules.size() >= 99) {
        return;
    }
    m_rules.append(rule);
    rebuildList();
    m_list->setCurrentRow(m_rules.size() - 1);
    m_pattern->setFocus();
    m_pattern->selectAll();
}

void HighlightsDialog::rebuildList()
{
    const bool loading = m_loading;
    m_loading = true;
    const int keep = m_list->currentRow();
    m_list->clear();
    for (const QuickHighlight &rule : std::as_const(m_rules)) {
        auto *item = new QListWidgetItem(rule.caption(), m_list);
        item->setFlags(item->flags() | Qt::ItemIsUserCheckable | Qt::ItemIsDragEnabled);
        item->setCheckState(rule.enabled ? Qt::Checked : Qt::Unchecked);
        item->setToolTip(rule.describe());
        // A swatch, so the list is readable without opening each row.
        if (rule.fore.isValid()) {
            item->setForeground(rule.fore);
        }
        if (rule.back.isValid()) {
            item->setBackground(rule.back);
        }
    }
    if (keep >= 0 && keep < m_list->count()) {
        m_list->setCurrentRow(keep);
    }
    m_loading = loading;
}

void HighlightsDialog::load(int row)
{
    m_loading = true;
    m_current = row;
    const bool have = row >= 0 && row < m_rules.size();
    m_fields->setEnabled(have);
    m_duplicate->setEnabled(have);
    m_remove->setEnabled(have);

    const QuickHighlight rule = have ? m_rules.at(row) : QuickHighlight();
    m_label->setText(rule.label);
    m_pattern->setText(rule.pattern);
    m_literal->setChecked(rule.literal);
    m_ignoreCase->setChecked(rule.ignoreCase);
    m_foreOn->setChecked(rule.fore.isValid());
    paintColorButton(m_fore, rule.fore);
    m_fore->setEnabled(rule.fore.isValid());
    m_backOn->setChecked(rule.back.isValid());
    paintColorButton(m_back, rule.back);
    m_back->setEnabled(rule.back.isValid());
    m_bold->setChecked((rule.style & TT_HIGHLIGHT_BOLD) != 0);
    m_underline->setChecked((rule.style & TT_HIGHLIGHT_UNDERLINE) != 0);
    m_reverse->setChecked((rule.style & TT_HIGHLIGHT_REVERSE) != 0);
    m_wholeLine->setChecked(rule.wholeLine);
    m_group->setValue(int(rule.group));
    m_loading = false;
    refreshPreview();
}

void HighlightsDialog::commit()
{
    if (m_loading || m_current < 0 || m_current >= m_rules.size()) {
        return;
    }
    QuickHighlight &rule = m_rules[m_current];
    rule.label = m_label->text();
    rule.pattern = m_pattern->text();
    rule.literal = m_literal->isChecked();
    rule.ignoreCase = m_ignoreCase->isChecked();

    // The check box owns whether there is a colour at all; the button owns
    // which one. Turning the box on with no colour chosen yet starts from the
    // terminal's own, so the picker opens somewhere sensible.
    m_fore->setEnabled(m_foreOn->isChecked());
    m_back->setEnabled(m_backOn->isChecked());
    QColor fore = colorOf(m_fore);
    if (m_foreOn->isChecked() && !fore.isValid()) {
        fore = palette().color(QPalette::WindowText);
    }
    QColor back = colorOf(m_back);
    if (m_backOn->isChecked() && !back.isValid()) {
        back = palette().color(QPalette::Window);
    }
    rule.fore = m_foreOn->isChecked() ? fore : QColor();
    rule.back = m_backOn->isChecked() ? back : QColor();
    paintColorButton(m_fore, rule.fore);
    paintColorButton(m_back, rule.back);

    rule.style = 0;
    if (m_bold->isChecked()) {
        rule.style |= TT_HIGHLIGHT_BOLD;
    }
    if (m_underline->isChecked()) {
        rule.style |= TT_HIGHLIGHT_UNDERLINE;
    }
    if (m_reverse->isChecked()) {
        rule.style |= TT_HIGHLIGHT_REVERSE;
    }
    rule.wholeLine = m_wholeLine->isChecked();
    rule.group = quint32(m_group->value());

    if (auto *item = m_list->item(m_current)) {
        const bool loading = m_loading;
        m_loading = true;
        item->setText(rule.caption());
        item->setToolTip(rule.describe());
        item->setForeground(rule.fore.isValid() ? QBrush(rule.fore) : QBrush());
        item->setBackground(rule.back.isValid() ? QBrush(rule.back) : QBrush());
        m_loading = loading;
    }
    refreshPreview();
}

bool HighlightsDialog::validatePatterns()
{
    for (int row = 0; row < m_rules.size(); row++) {
        const QuickHighlight &rule = m_rules.at(row);
        if (rule.pattern.isEmpty()) {
            continue;
        }
        QString error;
        if (checkHighlightPattern(rule.pattern, rule.literal, rule.ignoreCase, &error)) {
            continue;
        }
        // Bring the rejected rule into view. The field already shows the same
        // engine error while it is being typed; this covers an invalid rule on
        // another row when OK is pressed.
        m_list->setCurrentRow(row);
        m_patternError->setText(error);
        m_pattern->setFocus();
        return false;
    }
    return true;
}

void HighlightsDialog::refreshPreview()
{
    // The engine's own verdict on the pattern, as it is typed. Empty is not an
    // error — it is a rule somebody has just added.
    QString error;
    const QString pattern = m_pattern->text();
    const bool bad = !pattern.isEmpty()
                     && !checkHighlightPattern(pattern, m_literal->isChecked(),
                                               m_ignoreCase->isChecked(), &error);
    m_patternError->setText(bad ? error : QString());

    const QString sample = m_sample->toPlainText().split(QLatin1Char('\n')).value(0);
    if (sample.isEmpty()) {
        m_preview->clear();
        return;
    }
    m_preview->setText(highlightPreviewHtml(m_rules, sample,
                                            palette().color(QPalette::WindowText),
                                            palette().color(QPalette::Window)));
}
