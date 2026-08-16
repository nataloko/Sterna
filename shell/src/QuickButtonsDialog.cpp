// Copyright (c) the Sterna authors. 3-clause BSD; see LICENSE.

#include "QuickButtonsDialog.h"

#include <algorithm>

#include <QAction>
#include <QCheckBox>
#include <QComboBox>
#include <QDialogButtonBox>
#include <QFileDialog>
#include <QFileInfo>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QLineEdit>
#include <QInputDialog>
#include <QListWidget>
#include <QMenu>
#include <QMessageBox>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QStackedWidget>
#include <QToolButton>
#include <QVBoxLayout>

#include "Session.h"
#include "TerminalView.h"

namespace {

/// The menu commands a button may invoke.
///
/// The ids are `tt_res.h`'s and the list is the subset `MainWindow::
/// invokeMenuCommand` actually implements — offering one it ignores would be
/// a button that does nothing, which is worse than not offering it. A user key
/// in a `KEYBOARD.CNF` can name any number; this is a picker, so it names the
/// ones that work.
struct MenuCommand {
    quint16 id;
    const char *label;
};

const MenuCommand kCommands[] = {
    {50430, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Send break")},
    {50120, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Log...")},
    {50124, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Pause or resume logging")},
    {50125, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Stop logging")},
    {50130, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Send file...")},
    {50131, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Receive file...")},
    {50210, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Copy")},
    {50230, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Paste")},
    {50240, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Paste and send")},
    {50110, QT_TRANSLATE_NOOP("QuickButtonsDialog", "New connection...")},
    {50111, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Duplicate session")},
    {50112, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Local shell")},
    {50190, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Disconnect")},
    {50310, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Terminal settings...")},
    {50330, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Font...")},
    {50380, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Save setup")},
    {50395, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Load key map...")},
    {50470, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Run macro...")},
    {50199, QT_TRANSLATE_NOOP("QuickButtonsDialog", "Close the window")},
};

/// The convenience set. `Ctrl+Alt+digit` because Alt alone is Meta when
/// `MetaKey` is on, Ctrl alone is how a terminal sends control characters, and
/// the function keys belong to the host.
QKeySequence defaultShortcut(int index)
{
    static const int keys[] = {Qt::Key_1, Qt::Key_2, Qt::Key_3, Qt::Key_4,
                               Qt::Key_5, Qt::Key_6, Qt::Key_7, Qt::Key_8,
                               Qt::Key_9, Qt::Key_0};
    if (index < 0 || index >= static_cast<int>(std::size(keys))) {
        return {};
    }
    return QKeySequence(Qt::CTRL | Qt::ALT | keys[index]);
}

} // namespace

QuickButtonsDialog::QuickButtonsDialog(const QuickButtonSet &set, int page,
                                       const Session *session,
                                       const QWidget *window, QWidget *parent)
    : QDialog(parent), m_set(set), m_session(session), m_window(window)
{
    setObjectName(QStringLiteral("quickButtonsDialog"));
    setWindowTitle(tr("Quick buttons"));
    m_page = qBound(1, page, m_set.pageCount());

    // The page row, above the list it filters. The drop-down is greyed until
    // there is a second page — a control that can only say one thing — but the
    // row stays, because the Pages menu beside it is where the second page
    // comes from and hiding the whole row would hide the way in.
    m_pageRow = new QWidget(this);
    auto *pageLayout = new QHBoxLayout(m_pageRow);
    pageLayout->setContentsMargins(0, 0, 0, 0);
    m_pageList = new QComboBox(m_pageRow);
    m_pageList->setObjectName(QStringLiteral("quickButtonsPageList"));
    auto *pageMenuButton = new QToolButton(m_pageRow);
    pageMenuButton->setObjectName(QStringLiteral("quickButtonsPageMenu"));
    pageMenuButton->setText(tr("Pages"));
    pageMenuButton->setPopupMode(QToolButton::InstantPopup);
    auto *pageMenu = new QMenu(pageMenuButton);
    const auto pageAction = [this, pageMenu](const QString &text, const char *name,
                                             void (QuickButtonsDialog::*slot)()) {
        QAction *action = pageMenu->addAction(text);
        action->setObjectName(QString::fromLatin1(name));
        connect(action, &QAction::triggered, this, slot);
        return action;
    };
    pageAction(tr("Add page..."), "quickButtonsPageAdd", &QuickButtonsDialog::addPage);
    pageAction(tr("Rename page..."), "quickButtonsPageRename",
               &QuickButtonsDialog::renamePage);
    m_pageRemove = pageAction(tr("Remove page"), "quickButtonsPageRemove",
                              &QuickButtonsDialog::removePage);
    pageMenu->addSeparator();
    pageAction(tr("Import page..."), "quickButtonsPageImport",
               &QuickButtonsDialog::importPage);
    pageAction(tr("Export page..."), "quickButtonsPageExport",
               &QuickButtonsDialog::exportPage);
    pageMenuButton->setMenu(pageMenu);
    pageLayout->addWidget(new QLabel(tr("Page:"), m_pageRow));
    pageLayout->addWidget(m_pageList, 1);
    pageLayout->addWidget(pageMenuButton);

    m_list = new QListWidget(this);
    m_list->setObjectName(QStringLiteral("quickButtonsList"));
    // Drag to reorder, which is the order they appear on the bar. A pair of
    // up/down buttons would say the same thing in two more widgets.
    m_list->setDragDropMode(QAbstractItemView::InternalMove);
    m_list->setMinimumWidth(180);

    auto *add = new QPushButton(tr("Add"), this);
    add->setObjectName(QStringLiteral("quickButtonsAdd"));
    m_duplicate = new QPushButton(tr("Duplicate"), this);
    m_remove = new QPushButton(tr("Remove"), this);
    m_remove->setObjectName(QStringLiteral("quickButtonsRemove"));

    m_label = new QLineEdit(this);
    m_label->setObjectName(QStringLiteral("quickButtonLabel"));
    m_label->setPlaceholderText(tr("Shown on the button"));

    m_kind = new QComboBox(this);
    m_kind->setObjectName(QStringLiteral("quickButtonKind"));
    m_kind->addItem(tr("Send text"), TT_QUICK_BUTTON_TEXT);
    m_kind->addItem(tr("Send bytes"), TT_QUICK_BUTTON_BYTES);
    m_kind->addItem(tr("Run macro"), TT_QUICK_BUTTON_MACRO);
    m_kind->addItem(tr("Menu command"), TT_QUICK_BUTTON_COMMAND);

    // One field per kind rather than one field that means four things: a path
    // wants a Browse button and a command wants a list of the commands there
    // are, and neither is a line of text somebody should have to know the
    // spelling of.
    m_text = new QPlainTextEdit(this);
    m_text->setObjectName(QStringLiteral("quickButtonText"));
    m_text->setTabChangesFocus(true);
    m_text->setPlaceholderText(tr("What to send"));

    auto *pathRow = new QWidget(this);
    auto *pathLayout = new QHBoxLayout(pathRow);
    pathLayout->setContentsMargins(0, 0, 0, 0);
    m_path = new QLineEdit(pathRow);
    m_path->setObjectName(QStringLiteral("quickButtonPath"));
    auto *browse = new QPushButton(tr("Browse..."), pathRow);
    pathLayout->addWidget(m_path);
    pathLayout->addWidget(browse);

    m_command = new QComboBox(this);
    m_command->setObjectName(QStringLiteral("quickButtonCommand"));
    for (const MenuCommand &c : kCommands) {
        m_command->addItem(tr(c.label), QString::number(c.id));
    }

    m_value = new QStackedWidget(this);
    m_value->addWidget(m_text);
    m_value->addWidget(pathRow);
    m_value->addWidget(m_command);

    m_enter = new QCheckBox(tr("Send Enter after"), this);
    m_enter->setObjectName(QStringLiteral("quickButtonEnter"));
    m_enter->setToolTip(tr("This option adds Enter after the button's text or bytes. "
                           "A Shift-click sends the same content without Enter."));

    // "Repeat: [10] times every [2.5] s" — one sentence on one row, because
    // the count and the cadence are one decision and reading them apart
    // invites a button that sends a hundred times a second.
    auto *repeatRow = new QWidget(this);
    auto *repeatLayout = new QHBoxLayout(repeatRow);
    repeatLayout->setContentsMargins(0, 0, 0, 0);
    m_repeat = new QSpinBox(repeatRow);
    m_repeat->setObjectName(QStringLiteral("quickButtonRepeat"));
    m_repeat->setRange(0, static_cast<int>(TT_QUICK_BUTTON_MAX_REPEAT));
    // Below one, because a run with no end is what is reached for when no
    // number is the right one. `QSpinBox` shows this in place of its minimum.
    m_repeat->setSpecialValueText(tr("Until stopped"));
    m_repeatTimes = new QLabel(repeatRow);
    m_every = new QLabel(tr("every"), repeatRow);
    m_interval = new QDoubleSpinBox(repeatRow);
    m_interval->setObjectName(QStringLiteral("quickButtonInterval"));
    m_interval->setDecimals(1);
    m_interval->setSingleStep(0.5);
    m_interval->setRange(TT_QUICK_BUTTON_MIN_INTERVAL_MS / 1000.0,
                         TT_QUICK_BUTTON_MAX_INTERVAL_MS / 1000.0);
    m_interval->setSuffix(tr(" s"));
    repeatLayout->addWidget(m_repeat);
    repeatLayout->addWidget(m_repeatTimes);
    repeatLayout->addWidget(m_every);
    repeatLayout->addWidget(m_interval);
    repeatLayout->addStretch();
    repeatRow->setToolTip(
        tr("A second button press stops the repeat. The Escape key in the terminal "
           "also stops the repeat. A lost connection stops the repeat."));

    m_shortcut = new QKeySequenceEdit(this);
    m_shortcut->setObjectName(QStringLiteral("quickButtonShortcut"));

    m_warning = new QLabel(this);
    m_warning->setObjectName(QStringLiteral("quickButtonWarning"));
    m_warning->setWordWrap(true);
    m_warning->setStyleSheet(QStringLiteral("QLabel { color: #b71c1c; }"));

    auto *standard = new QPushButton(tr("Assign Ctrl+Alt+1...0"), this);
    standard->setToolTip(
        tr("This command adds the Ctrl+Alt+1 thru Ctrl+Alt+0 shortcuts to buttons "
           "1 thru 10. The remote system will not receive these key combinations."));

    m_confirm = new QCheckBox(tr("Ask before running"), this);
    m_confirm->setObjectName(QStringLiteral("quickButtonConfirm"));

    // Which page this button is on. A field like any other, because moving a
    // button between pages is an edit and not a gesture — and because the
    // panel's own Move to page menu cannot reach a button that is not drawn.
    m_pageOf = new QComboBox(this);
    m_pageOf->setObjectName(QStringLiteral("quickButtonPage"));

    m_fields = new QWidget(this);
    auto *form = new QFormLayout(m_fields);
    form->setContentsMargins(0, 0, 0, 0);
    form->addRow(tr("Label:"), m_label);
    form->addRow(tr("On page:"), m_pageOf);
    form->addRow(tr("Does:"), m_kind);
    form->addRow(tr("Command:"), m_value);
    form->addRow(QString(), m_enter);
    form->addRow(tr("Repeat:"), repeatRow);
    form->addRow(tr("Shortcut:"), m_shortcut);
    form->addRow(QString(), m_warning);
    form->addRow(QString(), m_confirm);

    auto *buttonRow = new QHBoxLayout;
    buttonRow->addWidget(add);
    buttonRow->addWidget(m_duplicate);
    buttonRow->addWidget(m_remove);
    buttonRow->addStretch();

    auto *left = new QVBoxLayout;
    left->addWidget(m_pageRow);
    left->addWidget(m_list);
    left->addLayout(buttonRow);

    auto *right = new QVBoxLayout;
    right->addWidget(m_fields);
    right->addWidget(standard);
    right->addStretch();

    auto *columns = new QHBoxLayout;
    columns->addLayout(left, 1);
    columns->addLayout(right, 2);

    auto *box = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel,
                                     this);
    connect(box, &QDialogButtonBox::accepted, this, [this] {
        commit();
        accept();
    });
    connect(box, &QDialogButtonBox::rejected, this, &QDialog::reject);

    auto *layout = new QVBoxLayout(this);
    layout->addLayout(columns);
    layout->addWidget(box);

    connect(m_list, &QListWidget::currentRowChanged, this, [this](int row) {
        if (!m_loading) {
            commit();
        }
        load(row);
    });
    // A drag reorders the model, not just the view.
    connect(m_list->model(), &QAbstractItemModel::rowsMoved, this,
            [this](const QModelIndex &, int from, int, const QModelIndex &, int to) {
                if (m_loading || from < 0 || from >= m_rows.size()) {
                    return;
                }
                // **Permute the slots this page occupies, do not move a button
                // through the whole list.** The global positions the page sits
                // in do not change; only which button is in each of them does.
                // That leaves every other page exactly where it was, and needs
                // no reasoning about which side of `from` the drop landed on
                // beyond Qt's own off-by-one.
                QVector<int> order = m_rows;
                order.move(from, to > from ? to - 1 : to);
                QVector<QuickButton> next = m_set.buttons;
                for (int k = 0; k < m_rows.size(); k++) {
                    next[m_rows[k]] = m_set.buttons[order[k]];
                }
                m_set.buttons = next;
                m_current = globalOf(m_list->currentRow());
            });
    connect(add, &QPushButton::clicked, this, &QuickButtonsDialog::addButton);
    connect(m_duplicate, &QPushButton::clicked, this,
            &QuickButtonsDialog::duplicateButton);
    connect(m_remove, &QPushButton::clicked, this,
            &QuickButtonsDialog::removeButton);
    connect(standard, &QPushButton::clicked, this,
            &QuickButtonsDialog::assignDefaultShortcuts);
    connect(browse, &QPushButton::clicked, this, [this] {
        const QString chosen = QFileDialog::getOpenFileName(
            this, tr("Macro"), QFileInfo(m_path->text()).absolutePath(),
            tr("Macros (*.ttl *.lua);;All files (*)"));
        if (!chosen.isEmpty()) {
            m_path->setText(chosen);
        }
    });

    // Every field writes back as it is typed, so the list's caption follows
    // the label and nothing has to be "applied" before switching rows.
    connect(m_label, &QLineEdit::textEdited, this, [this] {
        commit();
        const int row = rowOf(m_current);
        if (row >= 0 && row < m_list->count()) {
            m_list->item(row)->setText(m_set.buttons[m_current].caption());
        }
    });
    connect(m_kind, &QComboBox::currentIndexChanged, this, [this] {
        applyKind();
        commit();
    });
    connect(m_text, &QPlainTextEdit::textChanged, this, [this] { commit(); });
    connect(m_path, &QLineEdit::textChanged, this, [this] { commit(); });
    connect(m_command, &QComboBox::currentIndexChanged, this, [this] { commit(); });
    connect(m_enter, &QCheckBox::toggled, this, [this] { commit(); });
    connect(m_confirm, &QCheckBox::toggled, this, [this] { commit(); });
    connect(m_repeat, &QSpinBox::valueChanged, this, [this] {
        applyRepeat();
        commit();
    });
    connect(m_interval, &QDoubleSpinBox::valueChanged, this, [this] { commit(); });
    connect(m_shortcut, &QKeySequenceEdit::keySequenceChanged, this, [this] {
        checkShortcut();
        commit();
    });

    connect(m_pageList, &QComboBox::currentIndexChanged, this, [this](int index) {
        // **The whole body is guarded, not only the commit.** `rebuildPages`
        // fills this box, and the *first* `addItem` emits
        // `currentIndexChanged(0)` — so an unguarded `setPage(1)` here ran
        // inside the constructor and threw away the page the panel asked the
        // editor to open on. The symptom was Remove page removing the wrong
        // one.
        if (m_loading) {
            return;
        }
        commit();
        setPage(index + 1);
    });
    // Moving a button between pages: an edit like any other, so it commits and
    // the list follows. The row leaves this page's list, which is legible
    // because the list is visibly one page's.
    connect(m_pageOf, &QComboBox::currentIndexChanged, this, [this] {
        if (m_loading || m_current < 0 || m_current >= m_set.buttons.size()) {
            return;
        }
        // **A cleared box has no data, and `toInt()` on nothing is 0** — a page
        // number no button may hold, which would take it off every page in this
        // dialog and out of an export. `clear()` emits `currentIndexChanged(-1)`,
        // so this is one leaked `m_loading` away from being reachable.
        const QVariant page = m_pageOf->currentData();
        if (!page.isValid()) {
            return;
        }
        const int moved = m_current;
        m_set.buttons[moved].page = static_cast<quint32>(page.toInt());
        commit();
        rebuildList();
        // Follow it, rather than leaving the fields showing a button that is no
        // longer in the list beside them.
        setPage(static_cast<int>(m_set.buttons[moved].page));
        m_list->setCurrentRow(rowOf(moved));
    });

    rebuildPages();
    rebuildList();
    if (m_set.buttons.isEmpty()) {
        load(-1);
    } else {
        // Through the list rather than straight to `load`, so the row the
        // fields are showing is also the row that looks selected.
        m_list->setCurrentRow(0);
    }
}

void QuickButtonsDialog::rebuildList()
{
    const bool wasLoading = m_loading;
    m_loading = true;
    const int keep = m_list->currentRow();
    m_list->clear();
    // The rows on show, and where each of them lives in the whole list. This
    // is the only place the filtered numbering and the global one meet.
    m_rows.clear();
    for (int i = 0; i < m_set.buttons.size(); i++) {
        if (static_cast<int>(m_set.buttons[i].page) != m_page) {
            continue;
        }
        m_rows.append(i);
        m_list->addItem(m_set.buttons[i].caption());
    }
    if (keep >= 0 && keep < m_list->count()) {
        m_list->setCurrentRow(keep);
    }
    m_loading = wasLoading;
    applyPageControls();
}

void QuickButtonsDialog::selectRow(int index)
{
    if (index < 0 || index >= m_set.buttons.size()) {
        return;
    }
    // Follow the button onto its own page, rather than showing nothing and
    // leaving somebody to find where the thing they asked to edit went.
    setPage(static_cast<int>(m_set.buttons[index].page));
    m_list->setCurrentRow(rowOf(index));
}

void QuickButtonsDialog::appendButton(const QuickButton &seed)
{
    m_set.buttons.append(seed);
    const int added = m_set.buttons.size() - 1;
    setPage(static_cast<int>(seed.page));
    rebuildList();
    m_list->setCurrentRow(rowOf(added));
    load(rowOf(added));
    m_label->setFocus();
}

void QuickButtonsDialog::commit()
{
    if (m_loading || m_current < 0 || m_current >= m_set.buttons.size()) {
        return;
    }
    QuickButton &button = m_set.buttons[m_current];
    button.label = m_label->text();
    button.kind = static_cast<TtQuickButtonKind>(m_kind->currentData().toUInt());
    switch (button.kind) {
    case TT_QUICK_BUTTON_MACRO:
        button.text = m_path->text();
        break;
    case TT_QUICK_BUTTON_COMMAND:
        button.text = m_command->currentData().toString();
        break;
    default:
        // The box holds line feeds because that is what a text edit produces;
        // a terminal wants carriage returns, and this is the boundary where
        // the difference belongs. See `Vt::encode_text`.
        button.text = m_text->toPlainText().replace(QLatin1Char('\n'),
                                                    QLatin1Char('\r'));
        if (m_enter->isChecked() && !button.text.endsWith(QLatin1Char('\r'))) {
            button.text += QLatin1Char('\r');
        } else if (!m_enter->isChecked()) {
            while (button.text.endsWith(QLatin1Char('\r'))) {
                button.text.chop(1);
            }
        }
        break;
    }
    button.shortcut = m_shortcut->keySequence().toString(QKeySequence::PortableText);
    button.confirm = m_confirm->isChecked();
    button.repeat = m_repeat->value() == 0
        ? TT_QUICK_BUTTON_REPEAT_FOREVER
        : static_cast<quint32>(m_repeat->value());
    // Seconds on screen, milliseconds in the file: one decimal place is what
    // this is asked for in and rounding here is what keeps 2.5 from becoming
    // 2499.
    button.intervalMs =
        static_cast<quint32>(qRound(m_interval->value() * 1000.0));
    // `value` is the core's to produce; it is written when the window saves.
    button.value.clear();
}

void QuickButtonsDialog::load(int row)
{
    m_loading = true;
    m_current = globalOf(row);
    const bool has = m_current >= 0 && m_current < m_set.buttons.size();
    m_fields->setEnabled(has);
    m_duplicate->setEnabled(has);
    m_remove->setEnabled(has);
    if (has) {
        const QuickButton &button = m_set.buttons[m_current];
        // `int`, matching what `rebuildPages` put in: `findData` compares
        // `QVariant`s, and a `quint32` against a stored `int` is a comparison
        // that need not hold — the failure would be silent, showing the wrong
        // page in a field whose next edit then moves the button there.
        const int pageRow = m_pageOf->findData(static_cast<int>(button.page));
        m_pageOf->setCurrentIndex(pageRow < 0 ? m_page - 1 : pageRow);
        m_label->setText(button.label);
        const int kindRow = m_kind->findData(button.kind);
        m_kind->setCurrentIndex(kindRow < 0 ? 0 : kindRow);
        QString text = button.text;
        m_enter->setChecked(button.sendsEnter());
        if (button.sendsEnter()) {
            text.chop(1);
        }
        m_text->setPlainText(QString(text).replace(QLatin1Char('\r'),
                                                   QLatin1Char('\n')));
        m_path->setText(button.text);
        int commandRow = m_command->findData(button.text);
        if (button.kind == TT_QUICK_BUTTON_COMMAND && commandRow < 0) {
            // The file may name a command this window does not offer in its
            // picker. Keep it visible and, most importantly, unchanged when
            // the dialog is accepted for some other edit.
            m_command->addItem(tr("Command %1").arg(button.text), button.text);
            commandRow = m_command->count() - 1;
        }
        m_command->setCurrentIndex(commandRow < 0 ? 0 : commandRow);
        m_shortcut->setKeySequence(
            QKeySequence::fromString(button.shortcut, QKeySequence::PortableText));
        m_confirm->setChecked(button.confirm);
        m_repeat->setValue(button.repeatsForever()
                               ? 0
                               : static_cast<int>(button.repeat));
        m_interval->setValue(button.intervalMs / 1000.0);
        applyKind();
        applyRepeat();
    } else {
        m_label->clear();
        m_text->clear();
        m_path->clear();
        m_shortcut->clear();
        m_confirm->setChecked(false);
        m_enter->setChecked(false);
        m_repeat->setValue(1);
        m_interval->setValue(1.0);
        applyRepeat();
    }
    m_loading = false;
    checkShortcut();
}

void QuickButtonsDialog::applyKind()
{
    const auto kind = static_cast<TtQuickButtonKind>(m_kind->currentData().toUInt());
    switch (kind) {
    case TT_QUICK_BUTTON_MACRO:
        m_value->setCurrentIndex(1);
        break;
    case TT_QUICK_BUTTON_COMMAND:
        m_value->setCurrentIndex(2);
        break;
    default:
        m_value->setCurrentIndex(0);
        break;
    }
    // Only the two sending kinds have a line ending to add.
    m_enter->setVisible(kind == TT_QUICK_BUTTON_TEXT
                        || kind == TT_QUICK_BUTTON_BYTES);
}

void QuickButtonsDialog::applyRepeat()
{
    const int times = m_repeat->value();
    // Nothing to agree with when the spin box is showing "Until stopped".
    m_repeatTimes->setVisible(times != 0);
    m_repeatTimes->setText(times == 1 ? tr("time") : tr("times"));
    // An interval belongs to a repeat: on a button that sends once it is a
    // number that does nothing, and a number that does nothing is a number
    // somebody will spend an afternoon believing in.
    const bool repeating = times != 1;
    m_every->setVisible(repeating);
    m_interval->setVisible(repeating);
}

void QuickButtonsDialog::addButton()
{
    commit();
    QuickButton made;
    made.kind = TT_QUICK_BUTTON_TEXT;
    // On the page being looked at, which is the one the Add button is under.
    made.page = static_cast<quint32>(m_page);
    appendButton(made);
}

void QuickButtonsDialog::duplicateButton()
{
    commit();
    if (m_current < 0 || m_current >= m_set.buttons.size()) {
        return;
    }
    QuickButton copy = m_set.buttons[m_current];
    // Not the shortcut: two buttons cannot hold the same key, and taking it
    // from the original silently is the wrong half of that to choose. The page
    // *is* kept — a copy belongs beside what it was copied from.
    copy.shortcut.clear();
    const int at = m_current + 1;
    m_set.buttons.insert(at, copy);
    rebuildList();
    m_list->setCurrentRow(rowOf(at));
    load(rowOf(at));
}

void QuickButtonsDialog::removeButton()
{
    if (m_current < 0 || m_current >= m_set.buttons.size()) {
        return;
    }
    const int row = rowOf(m_current);
    m_set.buttons.remove(m_current);
    m_current = -1;
    rebuildList();
    const int next = qMin(row, m_list->count() - 1);
    m_list->setCurrentRow(next);
    load(next);
}

void QuickButtonsDialog::assignDefaultShortcuts()
{
    commit();
    // Across every page. A shortcut belongs to the window, so numbering these
    // per page would hand `Ctrl+Alt+1` to as many buttons as there are pages.
    for (int i = 0; i < m_set.buttons.size(); i++) {
        const QKeySequence sequence = defaultShortcut(i);
        if (sequence.isEmpty()) {
            break;
        }
        m_set.buttons[i].shortcut = sequence.toString(QKeySequence::PortableText);
    }
    load(rowOf(m_current));
}

QString QuickButtonsDialog::shortcutComplaint(const QKeySequence &sequence,
                                              int forRow) const
{
    if (sequence.isEmpty()) {
        return {};
    }

    for (int i = 0; i < m_set.buttons.size(); i++) {
        if (i == forRow || m_set.buttons[i].shortcut.isEmpty()) {
            continue;
        }
        if (QKeySequence::fromString(m_set.buttons[i].shortcut,
                                     QKeySequence::PortableText)
            == sequence) {
            // **Named with its page.** With pages the button in the way is
            // routinely one that is not on screen, and "already used by a
            // button you cannot see" reads as a fault in the dialog.
            if (m_set.pageCount() > 1) {
                return tr("Already used by the quick button \"%1\" on %2.")
                    .arg(m_set.buttons[i].caption(),
                         m_set.pageLabel(static_cast<int>(m_set.buttons[i].page)));
            }
            return tr("Already used by the quick button \"%1\".")
                .arg(m_set.buttons[i].caption());
        }
    }

    // The window's own menu items and anything a Lua plugin installed. Both
    // are ordinary `QAction`s on the window, which is why one walk finds them.
    if (m_window) {
        for (const QAction *action : m_window->findChildren<const QAction *>()) {
            if (action->shortcut() == sequence && !action->shortcut().isEmpty()
                && !action->objectName().startsWith(
                    QLatin1String("quickButton"))) {
                return tr("Already used by \"%1\".")
                    .arg(action->text().remove(QLatin1Char('&')));
            }
        }
    }

    // ...and the one nothing else can see: a key the *host* is expecting.
    // A QAction beats the terminal widget, so this would silently stop the
    // sequence reaching the far end.
    const quint16 scan = TerminalView::scanForSequence(sequence);
    if (scan != 0 && m_session && m_session->keyCodeBound(scan)) {
        return tr("The key map already assigns this key.");
    }

    const Qt::KeyboardModifiers mods = sequence[0].keyboardModifiers();
    const bool bare = (mods & ~Qt::ShiftModifier) == Qt::NoModifier;
    const int key = sequence[0].key();
    const bool function = key >= Qt::Key_F1 && key <= Qt::Key_F35;
    if (bare || function) {
        return tr("The terminal normally sends this key to the host, which "
                  "will stop while this button holds it.");
    }
    return {};
}

void QuickButtonsDialog::checkShortcut()
{
    const QString complaint =
        shortcutComplaint(m_shortcut->keySequence(), m_current);
    m_warning->setText(complaint);
    m_warning->setVisible(!complaint.isEmpty());
}

// --- pages -----------------------------------------------------------------

void QuickButtonsDialog::rebuildPages()
{
    const bool wasLoading = m_loading;
    m_loading = true;
    m_pageList->clear();
    m_pageOf->clear();
    for (int p = 1; p <= m_set.pageCount(); p++) {
        m_pageList->addItem(m_set.pageLabel(p));
        m_pageOf->addItem(m_set.pageLabel(p), p);
    }
    m_page = qBound(1, m_page, m_set.pageCount());
    m_pageList->setCurrentIndex(m_page - 1);
    m_loading = wasLoading;
    applyPageControls();
}

void QuickButtonsDialog::applyPageControls()
{
    // A page control that can only say one thing is greyed rather than hidden:
    // this is the editor, where finding out that pages exist is part of the
    // point, and Pages > Add page... beside it stays live. The *panel* keeps
    // the stricter rule — no drop-down at all until there is a second page —
    // because that one is chrome beside a terminal.
    const bool several = m_set.pageCount() > 1;
    m_pageList->setEnabled(several);
    m_pageOf->setEnabled(several);
    if (m_pageRemove) {
        m_pageRemove->setEnabled(several);
    }
}

void QuickButtonsDialog::setPage(int page)
{
    const int wanted = qBound(1, page, m_set.pageCount());
    if (wanted == m_page) {
        return;
    }
    const bool wasLoading = m_loading;
    m_page = wanted;
    m_loading = true;
    m_pageList->setCurrentIndex(m_page - 1);
    m_loading = wasLoading;
    rebuildList();
    // Nothing on this page is selected yet, so the fields show the first of it
    // — or nothing at all, on a page waiting for its first command.
    m_list->setCurrentRow(m_list->count() > 0 ? 0 : -1);
    load(m_list->currentRow());
    // **`load` ends by clearing `m_loading`**, so calling it from here would
    // otherwise leak the flag off for whoever was mid-rebuild — and the next
    // `clear()` on a combo would reach a handler that should have been quiet.
    m_loading = wasLoading;
}

void QuickButtonsDialog::addPage()
{
    commit();
    const int made = m_set.pageCount() + 1;
    bool chosen = false;
    const QString name = QInputDialog::getText(
        this, tr("Add page"), tr("Page name:"), QLineEdit::Normal,
        tr("Page %1").arg(made), &chosen);
    if (!chosen) {
        return;
    }
    // **A name, always.** A page is what a button says it is on, plus what has
    // been named — so an unnamed page with nothing on it is not something the
    // file can hold, and this one would be gone by the time it was saved.
    m_set.pageNames.resize(made);
    m_set.pageNames[made - 1] =
        name.trimmed().isEmpty() ? tr("Page %1").arg(made) : name.trimmed();
    rebuildPages();
    setPage(made);
}

void QuickButtonsDialog::renamePage()
{
    commit();
    bool chosen = false;
    const QString name = QInputDialog::getText(
        this, tr("Rename page"), tr("Page name:"), QLineEdit::Normal,
        m_set.pageNames.value(m_page - 1), &chosen);
    if (!chosen) {
        return;
    }
    if (m_set.pageNames.size() < m_page) {
        m_set.pageNames.resize(m_page);
    }
    // An empty name is not an error — it takes the page back to being called
    // by its number, and the key goes out of the file.
    m_set.pageNames[m_page - 1] = name.trimmed();
    // ...except on the last page, where the name is the only thing keeping it:
    // clearing it would delete a page somebody only wanted renamed.
    if (m_set.pageNames[m_page - 1].isEmpty() && m_page == m_set.pageCount()
        && m_set.pageCount() > 1) {
        const bool empty = std::none_of(
            m_set.buttons.cbegin(), m_set.buttons.cend(),
            [this](const QuickButton &b) { return static_cast<int>(b.page) == m_page; });
        if (empty) {
            m_set.pageNames[m_page - 1] = tr("Page %1").arg(m_page);
        }
    }
    rebuildPages();
    rebuildList();
}

void QuickButtonsDialog::removePage()
{
    commit();
    if (m_set.pageCount() <= 1) {
        return;
    }
    // **No question in front of it, because nothing is lost.** The commands
    // move to the page beside this one; removing a command is Remove, which
    // asks. The rule for where the pages above end up is the core's.
    const int gone = m_page;
    m_set = removeQuickButtonPage(m_set, gone);
    m_current = -1;
    m_page = qBound(1, gone > 1 ? gone - 1 : 1, m_set.pageCount());
    rebuildPages();
    rebuildList();
    m_list->setCurrentRow(m_list->count() > 0 ? 0 : -1);
    load(m_list->currentRow());
}

void QuickButtonsDialog::exportPage()
{
    commit();
    const QString chosen = QFileDialog::getSaveFileName(
        this, tr("Export page"), m_set.pageLabel(m_page) + QStringLiteral(".ini"),
        tr("Settings files (*.ini);;All files (*)"));
    if (chosen.isEmpty()) {
        return;
    }
    // **An exported page is an ordinary settings file.** One `[Sterna Buttons]`
    // section, its buttons on the first page so no `Page` key is written, and
    // the name in `Page1Name`. So it can be pasted into a settings file by
    // hand, a settings file can be imported as a page, and exporting onto a
    // file that already exists replaces that one section and leaves the rest of
    // it alone — which is the same operation as "put these buttons in the
    // router.ini I already have".
    QuickButtonSet out;
    for (const QuickButton &button : m_set.buttons) {
        if (static_cast<int>(button.page) != m_page) {
            continue;
        }
        QuickButton copy = button;
        copy.page = 1;
        out.buttons.append(copy);
    }
    const QString name = m_set.pageNames.value(m_page - 1);
    if (!name.isEmpty()) {
        out.pageNames.append(name);
    }
    QString error;
    if (!saveQuickButtons(chosen, out, &error)) {
        QMessageBox::warning(this, tr("Export page"),
                             tr("Could not export the page: %1").arg(error));
    }
}

void QuickButtonsDialog::importPage()
{
    commit();
    const QString chosen = QFileDialog::getOpenFileName(
        this, tr("Import page"), QString(),
        tr("Settings files (*.ini);;All files (*)"));
    if (chosen.isEmpty()) {
        return;
    }
    const QuickButtonSet in = loadQuickButtons(chosen);
    if (in.buttons.isEmpty()) {
        QMessageBox::warning(this, tr("Import page"),
                             tr("This file holds no quick buttons."));
        return;
    }
    // Refused rather than truncated: a silent half-import is the worst of the
    // three answers.
    if (m_set.buttons.size() + in.buttons.size() > TT_QUICK_BUTTON_MAX) {
        QMessageBox::warning(
            this, tr("Import page"),
            tr("A settings file holds %1 quick buttons, all pages together, and "
               "this import needs more.")
                .arg(TT_QUICK_BUTTON_MAX));
        return;
    }

    // **A file with several pages arrives as several pages**, one for one. Not
    // the first page only, which loses commands, and not flattened onto one,
    // which merges them — both silently.
    const int base = m_set.pageCount();
    for (const QuickButton &button : in.buttons) {
        QuickButton copy = button;
        copy.page = static_cast<quint32>(base + static_cast<int>(button.page));
        // Every shortcut, for Duplicate's reason: a key from another file would
        // be taken from whichever button in this one already had it, silently.
        copy.shortcut.clear();
        m_set.buttons.append(copy);
    }
    for (int p = 1; p <= in.pageCount(); p++) {
        const QString name = in.pageNames.value(p - 1);
        m_set.pageNames.resize(base + p);
        m_set.pageNames[base + p - 1] =
            name.isEmpty() ? QFileInfo(chosen).completeBaseName() : name;
    }
    rebuildPages();
    setPage(base + 1);
}
